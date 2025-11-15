use axum::{
    extract::{Path, State, Extension, ws::{WebSocketUpgrade, WebSocket, Message, Utf8Bytes}},
    response::IntoResponse,
    http::{HeaderMap, StatusCode, header},
};
use axum::body::Bytes;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::RwLock;
use tracing::{info, warn, error};
use im_share::{ImMqtt, MqttConfig, mqtt_user_topic, get_user_info_by_subscription, RedisClient, verify_token, JwtSettings};
use once_cell::sync::Lazy;

#[derive(Clone)]
pub struct MqttConnectionInfo {
    pub host: String,
    pub port: u16,
}

pub async fn ws_handler(
    State(mqtt_info): State<MqttConnectionInfo>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(jwt_cfg): Extension<JwtSettings>,
    Path(subscription_id): Path<String>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    info!(%subscription_id, "收到 WebSocket 升级请求");
    
    // 从请求头获取 token
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|s| {
            if s.starts_with("Bearer ") {
                Some(s[7..].to_string())
            } else {
                None
            }
        });
    
    let token = match token {
        Some(t) => t,
        None => {
            warn!(%subscription_id, "WebSocket 升级请求缺少 Authorization token");
            return (StatusCode::UNAUTHORIZED, "缺少认证 token").into_response();
        }
    };
    
    // 验证 token
    let claims = match verify_token(&token, &jwt_cfg) {
        Ok(c) => c,
        Err(e) => {
            warn!(%subscription_id, error = %e, "WebSocket token 验证失败");
            return (StatusCode::UNAUTHORIZED, "无效的认证 token").into_response();
        }
    };
    
    info!(
        %subscription_id,
        user_id = %claims.user_id,
        is_open_id = %claims.is_open_id,
        "WebSocket token 验证成功"
    );
    
    // 从 token 中提取用户信息
    // 如果 token 中包含 open_id（is_open_id = true），直接从 token 获取，无需查询数据库
    // 这样可以避免不必要的数据库查询，提高性能
    let (user_mqtt_id, user_open_id) = if claims.is_open_id {
        // Token 中包含 open_id 的数字形式（雪花算法生成的）
        // 直接使用，无需查询数据库
        let open_id = claims.user_id.to_string();
        info!(
            %subscription_id,
            open_id = %open_id,
            mqtt_id = %claims.user_id,
            "从 token 直接获取用户信息（无需查询数据库）"
        );
        (claims.user_id, open_id)
    } else {
        // Token 中包含的是数据库 ID（向后兼容旧 token）
        // 需要通过 API 查询 open_id 和 mqtt_id
        warn!(
            %subscription_id,
            user_id = %claims.user_id,
            "Token 使用数据库 ID（旧格式），需要通过 subscription_id 查询用户信息"
        );
        let server_url = std::env::var("IM_SERVER_URL")
            .unwrap_or_else(|_| "http://127.0.0.1:3000".to_string());
        match get_user_info_by_subscription(&server_url, &subscription_id).await {
            Ok((mqtt_id, open_id)) => {
                info!(
                    %subscription_id,
                    open_id = %open_id,
                    mqtt_id = %mqtt_id,
                    "通过 API 查询获取用户信息（向后兼容旧 token）"
                );
                (mqtt_id, open_id)
            },
            Err(e) => {
                error!(%subscription_id, error = %e, "查询用户信息失败");
                return (StatusCode::INTERNAL_SERVER_ERROR, "查询用户信息失败").into_response();
            }
        }
    };
    
    ws.on_upgrade(move |socket| handle_websocket_connection(
        socket, 
        mqtt_info, 
        subscription_id, 
        redis_client,
        user_mqtt_id,
        user_open_id,
    ))
}

// 在线用户列表（user_id -> subscription_id 集合，支持多设备）
// 这是 im-connect 特有的，用于跟踪在线用户
pub static ONLINE_USERS: Lazy<Arc<RwLock<HashMap<u64, std::collections::HashSet<String>>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// 检查用户是否在线（是否有活跃的WebSocket连接和MQTT订阅）
#[allow(dead_code)]
pub async fn check_user_online_status(user_mqtt_id: u64) -> bool {
    let online_users = ONLINE_USERS.read().await;
    online_users.contains_key(&user_mqtt_id) && 
    !online_users.get(&user_mqtt_id).map(|s| s.is_empty()).unwrap_or(true)
}

/// 获取用户的订阅信息
#[allow(dead_code)]
pub async fn get_user_subscriptions(user_mqtt_id: u64) -> Vec<String> {
    let online_users = ONLINE_USERS.read().await;
    online_users
        .get(&user_mqtt_id)
        .map(|s| s.iter().cloned().collect())
        .unwrap_or_default()
}

async fn handle_websocket_connection(
    mut socket: WebSocket,
    mqtt_info: MqttConnectionInfo,
    subscription_id: String,
    redis_client: Arc<RedisClient>,
    user_mqtt_id: u64,
    user_open_id: String,
) {
    
    // 使用基于 user_mqtt_id (snowflake_id) 的固定 client_id，确保同一用户的会话可以恢复
    // 这样 MQTT broker 可以为离线用户存储消息
    // 注意：使用 user_mqtt_id (snowflake_id) 而不是 subscription_id，因为：
    // 1. subscription_id 每次连接都会变化（每次登录生成新的）
    // 2. user_mqtt_id (snowflake_id) 和 open_id 是用户唯一且不变的标识符
    // 3. 如果同一用户有多个设备，它们会共享同一个 MQTT 会话，broker 会推送消息给所有连接的设备
    let client_id = format!("im-conn-{}", user_mqtt_id);
    info!(
        subscription_id = %subscription_id, 
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        client_id = %client_id, 
        "创建MQTT客户端（使用固定client_id以支持离线消息，基于open_id/mqtt_id而非subscription_id）"
    );
    let im = Arc::new(ImMqtt::connect(MqttConfig::new(
        mqtt_info.host.clone(),
        mqtt_info.port,
        client_id.clone(),
    )));
    
    info!(
        subscription_id = %subscription_id,
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        client_id = %client_id, 
        "为用户创建独立的MQTT客户端（基于唯一标识符open_id）"
    );
    
    let topic = mqtt_user_topic(&user_mqtt_id.to_string());
    
    // 保存 topic 用于后续取消订阅
    let topic_for_unsubscribe = topic.clone();
    
    info!(
        subscription_id = %subscription_id,
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        %topic, 
        client_id = %client_id, 
        "准备订阅MQTT topic（基于唯一标识符mqtt_id）"
    );
    
    let mut rx = match im.subscribe(&topic).await {
        Ok(r) => {
            info!(
                subscription_id = %subscription_id,
                open_id = %user_open_id,
                mqtt_id = %user_mqtt_id, 
                topic = %topic,
                client_id = %client_id,
                "✅ MQTT订阅成功（QoS 1），等待broker推送消息（包括离线消息，基于唯一标识符open_id）"
            );
            
            // 注意：subscribe 方法返回的 Receiver 表示已成功订阅
            // 如果返回了 Receiver，说明订阅成功，broadcast channel 中已经有接收者
            info!(
                subscription_id = %subscription_id,
                open_id = %user_open_id,
                mqtt_id = %user_mqtt_id,
                topic = %topic,
                "MQTT订阅确认：已获得 broadcast channel 接收者，可以接收消息"
            );
            
            // 等待一小段时间，让broker有时间推送离线消息
            // 注意：这不是必需的，因为broker会在订阅确认后立即推送离线消息
            // 但添加这个延迟可以帮助调试，确保订阅完全建立
            // 同时，broker推送离线消息可能需要一些时间
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
            info!(
                subscription_id = %subscription_id,
                mqtt_id = %user_mqtt_id,
                topic = %topic,
                "开始监听MQTT消息（broker应该已经推送了离线消息，如果有的话；注意：只有订阅后发布的消息才会被broker存储）"
            );
            
            r
        },
        Err(e) => {
            error!(
                subscription_id = %subscription_id,
                open_id = %user_open_id,
                mqtt_id = %user_mqtt_id, 
                topic = %topic,
                client_id = %client_id,
                error = %e, 
                "❌ MQTT订阅失败"
            );
            // 发送关闭帧并关闭连接
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    
    info!(
        subscription_id = %subscription_id,
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        %topic, 
        "WS已连接，已订阅MQTT（基于唯一标识符open_id，subscription_id仅用于本次连接）"
    );
    
    // 添加到在线用户列表
    {
        let mut online_users = ONLINE_USERS.write().await;
        online_users.entry(user_mqtt_id).or_insert_with(std::collections::HashSet::new).insert(subscription_id.clone());
        let subscription_count = online_users.get(&user_mqtt_id).map(|s| s.len()).unwrap_or(0);
        info!(
            subscription_id = %subscription_id,
            open_id = %user_open_id,
            mqtt_id = %user_mqtt_id,
            subscription_count = subscription_count,
            "用户已添加到在线列表（当前该用户有 {} 个活跃连接）",
            subscription_count
        );
    }
    
    // MQTT broker 会自动推送离线消息（使用 QoS 1 和 clean_session=false）
    // 当客户端重连并订阅 topic 后，broker 会自动推送离线期间的消息
    // 注意：使用固定的 client_id（基于 open_id/mqtt_id）确保会话可以恢复
    info!(
        subscription_id = %subscription_id,
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        %topic, 
        "已订阅MQTT topic，broker会自动推送离线消息（基于唯一标识符open_id，subscription_id每次连接都会变化）"
    );

    // 混合方案：从 Redis 获取离线消息（作为 MQTT 的补充）
    // MQTT 处理短期离线（用户曾经连接过），Redis 处理长期离线或从未连接的用户
    match redis_client.get_and_clear_offline_messages(&user_open_id).await {
        Ok(offline_messages) => {
            if !offline_messages.is_empty() {
                info!(
                    subscription_id = %subscription_id,
                    open_id = %user_open_id,
                    message_count = offline_messages.len(),
                    "开始推送 {} 条 Redis 离线消息",
                    offline_messages.len()
                );
                
                // 推送 Redis 中的离线消息
                for (index, message) in offline_messages.iter().enumerate() {
                    // 尝试解析消息以检查是否需要过滤
                    let should_skip = if let Ok(json) = serde_json::from_str::<serde_json::Value>(message) {
                        // 检查是否是通话邀请消息（语音/视频呼叫）
                        let is_call_invite = json.get("type")
                            .and_then(|t| t.as_str())
                            .map(|t| t == "call_invite")
                            .unwrap_or(false)
                            || json.get("message_content_type")
                                .and_then(|t| t.as_i64())
                                .map(|t| t == 4)
                                .unwrap_or(false);
                        
                        if is_call_invite {
                            // 检查通话邀请是否已过期
                            let now = chrono::Utc::now().timestamp_millis();
                            let message_timestamp = json.get("timestamp")
                                .or_else(|| json.get("timestamp_ms"))
                                .or_else(|| json.get("created_at"))
                                .and_then(|t| t.as_i64())
                                .or_else(|| {
                                    // 尝试从 message 字段中解析
                                    json.get("message")
                                        .and_then(|m| m.as_str())
                                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                                        .and_then(|msg_json| msg_json.get("timestamp").and_then(|t| t.as_i64()))
                                });
                            
                            let timeout = json.get("timeout")
                                .and_then(|t| t.as_i64())
                                .or_else(|| {
                                    // 尝试从 message 字段中解析
                                    json.get("message")
                                        .and_then(|m| m.as_str())
                                        .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
                                        .and_then(|msg_json| msg_json.get("timeout").and_then(|t| t.as_i64()))
                                })
                                .unwrap_or(60); // 默认60秒超时
                            
                            if let Some(ts) = message_timestamp {
                                let expire_time = ts + (timeout * 1000); // 超时时间转换为毫秒
                                
                                // 如果已过期，跳过这条消息
                                if now > expire_time {
                                    info!(
                                        subscription_id = %subscription_id,
                                        open_id = %user_open_id,
                                        message_index = index,
                                        message_timestamp = ts,
                                        timeout = timeout,
                                        expire_time = expire_time,
                                        now = now,
                                        expired_by_seconds = (now - expire_time) / 1000,
                                        "跳过已过期的通话邀请消息（不推送给客户端）"
                                    );
                                    true
                                } else {
                                    false
                                }
                            } else {
                                // 如果没有时间戳，为了安全，跳过这条消息（可能是历史消息）
                                info!(
                                    subscription_id = %subscription_id,
                                    open_id = %user_open_id,
                                    message_index = index,
                                    "跳过没有时间戳的通话邀请消息（可能是历史消息，不推送给客户端）"
                                );
                                true
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    
                    if should_skip {
                        continue; // 跳过这条消息，不推送
                    }
                    
                    // 尝试解析消息以检查 chat_type（用于日志）
                    let chat_type_info = if let Ok(json) = serde_json::from_str::<serde_json::Value>(message) {
                        format!("chat_type={:?}, from_user_id={:?}, to_user_id={:?}", 
                            json.get("chat_type"),
                            json.get("from_user_id"),
                            json.get("to_user_id"))
                    } else {
                        "无法解析JSON".to_string()
                    };
                    
                    info!(
                        subscription_id = %subscription_id,
                        open_id = %user_open_id,
                        message_index = index,
                        message_length = message.len(),
                        %chat_type_info,
                        message_preview = if message.len() > 100 { format!("{}...", &message[..100]) } else { message.clone() },
                        "准备推送 Redis 离线消息"
                    );
                    if let Err(e) = socket.send(Message::Text(Utf8Bytes::from(message.clone()))).await {
                        warn!(
                            subscription_id = %subscription_id,
                            open_id = %user_open_id,
                            message_index = index,
                            error = %e,
                            "推送 Redis 离线消息失败"
                        );
                        break; // 如果发送失败，停止推送剩余消息
                    } else {
                        info!(
                            subscription_id = %subscription_id,
                            open_id = %user_open_id,
                            message_index = index,
                            "✅ Redis 离线消息已发送到 WebSocket"
                        );
                    }
                }
                
                info!(
                    subscription_id = %subscription_id,
                    open_id = %user_open_id,
                    message_count = offline_messages.len(),
                    "✅ Redis 离线消息推送完成"
                );
            }
            // 没有离线消息时不输出日志（已在 redis.rs 中使用 debug 级别输出）
        }
        Err(e) => {
            warn!(
                subscription_id = %subscription_id,
                open_id = %user_open_id,
                error = %e,
                "从 Redis 获取离线消息失败（MQTT broker 仍会推送离线消息）"
            );
        }
    }

    // 定期发送 ping 保持连接活跃
    let mut ping_interval = tokio::time::interval(tokio::time::Duration::from_secs(30));
    ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    
    // 跟踪连接是否已经关闭，避免在已关闭的连接上发送关闭帧
    let mut connection_closed = false;
    
    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                // 定期发送 ping 保持连接活跃
                if let Err(e) = socket.send(Message::Ping(vec![].into())).await {
                    warn!(%subscription_id, user_id = %user_mqtt_id, error = %e, "发送 ping 失败");
                    connection_closed = true;
                    break;
                }
            }
            incoming = rx.recv() => {
                match incoming {
                    Ok(msg) => {
                        info!(
                            subscription_id = %subscription_id, 
                            open_id = %user_open_id,
                            mqtt_id = %user_mqtt_id, 
                            received_topic = %msg.topic, 
                            expected_topic = %topic, 
                            payload_len = msg.payload.len(),
                            "📨 收到MQTT消息（从broadcast channel）"
                        );
                        if msg.topic != topic {
                            warn!(
                                subscription_id = %subscription_id,
                                open_id = %user_open_id,
                                mqtt_id = %user_mqtt_id, 
                                received_topic = %msg.topic, 
                                expected_topic = %topic, 
                                "收到不匹配的topic消息，跳过（可能是订阅了多个topic）"
                            );
                            continue;
                        }
                        
                        // 尝试解析消息内容用于调试
                        let message_id = if let Ok(text) = String::from_utf8(msg.payload.clone()) {
                            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                let msg_id = json.get("message_id").and_then(|v| v.as_str()).map(|s| s.to_string());
                                info!(
                                    subscription_id = %subscription_id,
                                    mqtt_id = %user_mqtt_id,
                                    message_id = ?msg_id,
                                    chat_type = ?json.get("chat_type"),
                                    from_user_id = ?json.get("from_user_id"),
                                    to_user_id = ?json.get("to_user_id"),
                                    topic = %msg.topic,
                                    payload_len = msg.payload.len(),
                                    "✅ 处理MQTT消息（topic匹配，消息内容解析成功，准备发送到WebSocket客户端）"
                                );
                                msg_id
                            } else {
                                info!(
                                    subscription_id = %subscription_id,
                                    mqtt_id = %user_mqtt_id,
                                    topic = %msg.topic,
                                    payload_len = msg.payload.len(),
                                    payload_preview = %text.chars().take(100).collect::<String>(),
                                    "✅ 处理MQTT消息（topic匹配，但无法解析为JSON，准备发送到WebSocket客户端）"
                                );
                                None
                            }
                        } else {
                            info!(
                                subscription_id = %subscription_id,
                                mqtt_id = %user_mqtt_id,
                                topic = %msg.topic,
                                payload_len = msg.payload.len(),
                                "✅ 处理MQTT消息（topic匹配，二进制消息，准备发送到WebSocket客户端）"
                            );
                            None
                        };
                        
                        // 直接使用原始消息，不进行ID转换
                        // 前端可以处理 open_id，不需要转换为用户名
                        // 这样可以避免异步转换导致的延迟和错误
                        let payload = msg.payload;
                        let payload_len = payload.len();
                        
                        // 尝试解析消息内容用于日志
                        let message_text = if let Ok(text) = String::from_utf8(payload.clone()) {
                            Some(text)
                        } else {
                            None
                        };
                        
                        let send_result = match &message_text {
                            Some(text) => {
                                info!(
                                    subscription_id = %subscription_id,
                                    mqtt_id = %user_mqtt_id,
                                    message_id = ?message_id,
                                    message_len = text.len(),
                                    "发送文本消息到WebSocket客户端"
                                );
                                socket.send(Message::Text(Utf8Bytes::from(text.clone()))).await
                            },
                            None => {
                                info!(
                                    subscription_id = %subscription_id,
                                    mqtt_id = %user_mqtt_id,
                                    message_id = ?message_id,
                                    payload_len = payload_len,
                                    "发送二进制消息到WebSocket客户端"
                                );
                                socket.send(Message::Binary(Bytes::from(payload))).await
                            },
                        };

                        match send_result {
                            Ok(_) => {
                                info!(
                                    subscription_id = %subscription_id, 
                                    mqtt_id = %user_mqtt_id,
                                    message_id = ?message_id,
                                    payload_len = payload_len,
                                    "✅ 消息已成功发送到WebSocket客户端"
                                );
                            },
                            Err(e) => {
                                warn!(
                                    %subscription_id, 
                                    user_id = %user_mqtt_id, 
                                    error = %e, 
                                    "❌ 发送消息到客户端失败"
                                );
                                // 发送失败通常意味着连接已断开，退出循环
                                connection_closed = true;
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        // broadcast channel 错误通常表示：
                        // 1. 通道已关闭（所有发送者都关闭了）
                        // 2. 接收者滞后太多（消息积压超过256条）
                        let error_str = e.to_string();
                        if error_str.contains("channel closed") || error_str.contains("closed") {
                            warn!(
                                subscription_id = %subscription_id,
                                open_id = %user_open_id,
                                mqtt_id = %user_mqtt_id,
                                error = %e,
                                "MQTT broadcast channel 已关闭（MQTT连接可能已断开）"
                            );
                            connection_closed = true;
                            break;
                        } else {
                            warn!(
                                subscription_id = %subscription_id,
                                open_id = %user_open_id,
                                mqtt_id = %user_mqtt_id,
                                error = %e,
                                "MQTT接收通道错误（可能是消息积压，等待后重试）"
                            );
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                }
            }
            from_client = socket.recv() => {
                match from_client {
                    Some(Ok(Message::Close(_))) | None => {
                        info!(%subscription_id, user_id = %user_mqtt_id, open_id = %user_open_id, "WS关闭");
                        // 从在线用户列表移除
                        {
                            let mut online_users = ONLINE_USERS.write().await;
                            if let Some(subs) = online_users.get_mut(&user_mqtt_id) {
                                subs.remove(&subscription_id);
                                if subs.is_empty() {
                                    online_users.remove(&user_mqtt_id);
                                }
                            }
                        }
                        connection_closed = true;
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        // 收到 ping，回复 pong
                        if let Err(e) = socket.send(Message::Pong(data)).await {
                            warn!(%subscription_id, user_id = %user_mqtt_id, error = %e, "回复 pong 失败");
                            connection_closed = true;
                            break;
                        }
                    }
                    Some(Ok(Message::Pong(_))) => {
                        // 收到 pong，连接正常（客户端可能也在发送 ping）
                    }
                    Some(Ok(_)) => {
                        // 忽略其他客户端消息（仅保留服务端推送）
                    }
                    Some(Err(e)) => {
                        warn!(%subscription_id, user_id = %user_mqtt_id, open_id = %user_open_id, error = %e, "WS接收错误");
                        // 从在线用户列表移除
                        {
                            let mut online_users = ONLINE_USERS.write().await;
                            if let Some(subs) = online_users.get_mut(&user_mqtt_id) {
                                subs.remove(&subscription_id);
                                if subs.is_empty() {
                                    online_users.remove(&user_mqtt_id);
                                }
                            }
                        }
                        // 检查错误类型，如果是连接重置或已关闭，不需要发送关闭帧
                        let error_str = e.to_string();
                        let is_connection_reset = error_str.contains("Connection reset")
                            || error_str.contains("connection reset")
                            || error_str.contains("Broken pipe")
                            || error_str.contains("broken pipe")
                            || error_str.contains("Connection aborted")
                            || error_str.contains("connection aborted")
                            || error_str.contains("Sending after closing")
                            || error_str.contains("sending after closing");
                        
                        // 只有在连接仍然有效时才尝试发送关闭帧
                        if !is_connection_reset {
                            if let Err(close_err) = socket.send(Message::Close(None)).await {
                                // 如果发送关闭帧也失败，说明连接已经关闭
                                let close_err_str = close_err.to_string();
                                if close_err_str.contains("Sending after closing") 
                                    || close_err_str.contains("sending after closing") {
                                    connection_closed = true;
                                }
                            }
                        } else {
                            connection_closed = true;
                        }
                        break;
                    }
                }
            }
        }
    }
    
    // MQTT 连接断开说明：
    // 1. 当用户切换时（同一客户端不同用户），需要取消订阅以避免消息混乱
    // 2. 当同一用户重连时（例如网络断开重连），保留订阅以接收离线消息
    // 3. 使用 clean_session=false 时，broker 会保留会话状态
    // 4. 关键：使用固定的 client_id（基于 open_id/mqtt_id）确保会话可以恢复
    // 
    // 重要：对于多用户切换场景，我们需要取消订阅以确保：
    // - 用户A的消息不会发送到用户B
    // - 避免 MQTT broker 中的订阅信息混乱
    // - 下次同一用户重连时，会重新订阅，broker 会推送离线消息
    info!(
        subscription_id = %subscription_id,
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        "WebSocket 断开，准备清理 MQTT 连接和订阅"
    );
    
    // 取消订阅，避免多用户切换时的消息混乱
    // 注意：取消订阅会清除 broker 中的订阅信息
    // 但会话状态（包括离线消息）仍然保留（因为 clean_session=false）
    // 下次同一用户重连时，重新订阅后 broker 会推送离线消息
    if let Err(e) = im.unsubscribe(&topic_for_unsubscribe).await {
        warn!(
            subscription_id = %subscription_id,
            open_id = %user_open_id,
            mqtt_id = %user_mqtt_id,
            topic = %topic_for_unsubscribe,
            error = %e,
            "取消 MQTT 订阅失败（可能已经取消或连接已断开）"
        );
    } else {
        info!(
            subscription_id = %subscription_id,
            open_id = %user_open_id,
            mqtt_id = %user_mqtt_id,
            topic = %topic_for_unsubscribe,
            "✅ 已取消 MQTT 订阅（避免多用户切换时的消息混乱）"
        );
    }
    
    // 断开 MQTT 连接
    // 注意：rumqttc 的 AsyncClient 在 drop 时会自动断开连接
    // 但由于我们使用 Arc，需要确保引用计数为 0
    // 当 Arc 的引用计数为 0 时，MQTT 客户端会被 drop，连接会自动断开
    // 
    // 由于 im 是在函数作用域内创建的，当函数结束时，Arc 的引用计数会减少
    // 如果引用计数为 0，MQTT 客户端会被 drop，连接会自动断开
    // 事件循环会在连接断开时退出（在 mqtt.rs 的事件循环中处理）
    let _ = im.disconnect().await;
    
    // 释放 Arc 引用，触发 MQTT 客户端 drop
    drop(im);
    
    // 等待一小段时间，确保 MQTT 连接完全断开
    // 这对于多用户切换场景很重要，避免旧连接影响新连接
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
    
    info!(
        subscription_id = %subscription_id,
        open_id = %user_open_id,
        mqtt_id = %user_mqtt_id, 
        "MQTT 客户端已释放，连接已断开（已取消订阅，避免多用户切换时的消息混乱）"
    );
    
    // 确保从在线用户列表移除（双重保险）
    {
        let mut online_users = ONLINE_USERS.write().await;
        if let Some(subs) = online_users.get_mut(&user_mqtt_id) {
            subs.remove(&subscription_id);
            let remaining_count = subs.len();
            if subs.is_empty() {
                online_users.remove(&user_mqtt_id);
                info!(
                    subscription_id = %subscription_id,
                    open_id = %user_open_id,
                    mqtt_id = %user_mqtt_id,
                    "用户已从在线列表移除（该用户已无活跃连接）"
                );
            } else {
                info!(
                    subscription_id = %subscription_id,
                    open_id = %user_open_id,
                    mqtt_id = %user_mqtt_id,
                    remaining_connections = remaining_count,
                    "用户连接已移除（该用户仍有 {} 个活跃连接）",
                    remaining_count
                );
            }
        } else {
            warn!(
                subscription_id = %subscription_id,
                open_id = %user_open_id,
                mqtt_id = %user_mqtt_id,
                "用户不在在线列表中（可能已经被移除）"
            );
        }
    }
    
    // 尝试优雅关闭连接（如果连接仍然有效）
    // 注意：如果连接已经被重置或关闭，发送关闭帧可能会失败，这是正常的
    if !connection_closed {
        if let Err(e) = socket.send(Message::Close(None)).await {
            // 检查错误类型，如果是"发送后关闭"错误，不需要记录警告（这是预期的）
            let error_str = e.to_string();
            if !error_str.contains("Sending after closing") && !error_str.contains("sending after closing") {
                warn!(%subscription_id, user_id = %user_mqtt_id, error = %e, "发送关闭帧失败（连接可能已关闭）");
            }
        }
    }
    info!(%subscription_id, user_id = %user_mqtt_id, "WebSocket 连接已清理");
}


