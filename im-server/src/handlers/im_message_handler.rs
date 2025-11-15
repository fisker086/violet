use axum::{extract::{Path, Extension, State, Query}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, info, warn};
use im_share::{ChatMessage, mqtt_user_topic, encode_message};
use crate::{
    error::{ErrorCode, ErrorResponse},
    service::{ImMessageService, SubscriptionService, UserService, ImChatService, ImGroupService},
    model::{ImSingleMessage, ImGroupMessage},
    mqtt::MqttPublisher,
    redis::RedisClient,
};

#[derive(Deserialize)]
pub struct SendSingleMessageRequest {
    pub from_id: String,
    pub to_id: String,
    pub message_body: String,
    pub message_content_type: i32,
    pub extra: Option<String>,
    pub reply_to: Option<String>,
}

#[derive(Deserialize)]
pub struct SendGroupMessageRequest {
    pub group_id: String,
    pub from_id: String,
    pub message_body: String,
    pub message_content_type: i32,
    pub extra: Option<String>,
    pub reply_to: Option<String>,
}

pub async fn send_single_message(
    State((publisher, subscription_service)): State<(MqttPublisher, Arc<SubscriptionService>)>,
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(_user_id): Extension<u64>, // 从认证中间件获取用户ID
    Json(req): Json<SendSingleMessageRequest>,
) -> impl IntoResponse {
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    
    // 验证请求参数
    if req.from_id.is_empty() || req.to_id.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "from_id 和 to_id 不能为空")),
        ));
    }
    
    if req.message_body.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "消息内容不能为空")),
        ));
    }
    
    let service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    let user_service = UserService::new(pool.clone());
    
    // 先获取发送者和接收者的 open_id，确保统一使用 open_id
    // 发送者：优先使用 open_id 查找，如果失败则尝试作为用户名查找
    let from_user = match user_service.get_by_open_id(&req.from_id).await {
        Ok(user) => user,
        Err(_) => {
            // 作为用户名查找
            match user_service.get_by_name(&req.from_id).await {
                Ok(user) => user,
                Err(_) => {
                    warn!(from_id = %req.from_id, "无法找到发送者用户");
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::new(ErrorCode::NotFound, "发送者用户不存在")),
                    ));
                }
            }
        }
    };
    
    // 接收者：优先使用 open_id 查找，如果失败则尝试作为用户名查找
    let to_user = match user_service.get_by_open_id(&req.to_id).await {
        Ok(user) => user,
        Err(_) => {
            // 作为用户名查找
            match user_service.get_by_name(&req.to_id).await {
                Ok(user) => user,
                Err(_) => {
                    warn!(to_id = %req.to_id, "无法找到接收者用户");
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::new(ErrorCode::NotFound, "接收者用户不存在")),
                    ));
                }
            }
        }
    };
    
    // 统一使用 open_id 作为消息的 from_id 和 to_id
    let from_open_id = from_user.get_external_id();
    let to_open_id = to_user.get_external_id();
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    
    let message_id = Uuid::new_v4().to_string();
    
    // 保存消息到数据库（使用 open_id）
    let message = ImSingleMessage {
        message_id: message_id.clone(),
        from_id: from_open_id.clone(), // 使用 open_id
        to_id: to_open_id.clone(), // 使用 open_id
        message_body: req.message_body.clone(),
        message_time: now,
        message_content_type: req.message_content_type,
        read_status: 0,
        extra: req.extra.clone(),
        del_flag: 1,
        sequence: now, // 使用时间戳作为序列号
        message_random: Some(Uuid::new_v4().to_string()),
        create_time: Some(now),
        update_time: Some(now),
        version: Some(1),
        reply_to: req.reply_to.clone(),
        to_type: Some("User".to_string()),
        file_url: None,
        file_name: None,
        file_type: None,
    };
    
    // 保存消息到数据库
    match service.save_single_message(message).await {
        Ok(_) => {
            // 解析extra字段获取文件信息
            let mut file_url = None;
            let mut file_name = None;
            let mut file_type = None;
            
            if let Some(extra_str) = &req.extra {
                if let Ok(extra_json) = serde_json::from_str::<serde_json::Value>(extra_str) {
                    file_url = extra_json.get("file_url").and_then(|v| v.as_str()).map(|s| s.to_string());
                    file_name = extra_json.get("file_name").and_then(|v| v.as_str()).map(|s| s.to_string());
                    file_type = extra_json.get("file_type").and_then(|v| v.as_str()).map(|s| s.to_string());
                }
            }
            
            // 获取 open_id 的数字形式（用于MQTT）
            let to_mqtt_id = to_user.get_mqtt_id();
            
            // 将ImSingleMessage转换为ChatMessage格式用于MQTT推送
            // 使用 open_id 作为 from_user_id 和 to_user_id，确保ID格式一致
            let chat_message = ChatMessage {
                message_id: message_id.clone(),
                from_user_id: from_open_id.clone(), // 使用 open_id，确保ID格式一致
                to_user_id: to_open_id.clone(), // 使用 open_id
                message: req.message_body.clone(),
                timestamp_ms: now,
                file_url,
                file_name,
                file_type,
                chat_type: Some(1), // 1 = 单聊
            };
            
            // 从数据库查询订阅ID并同步到内存（如果内存中没有）
            let subscription_ids = {
                let mut ids = subscription_service.get_subscription_ids(to_user.id);
                if ids.is_empty() {
                    // 如果内存中没有，从数据库查询（只查询最近24小时内创建的订阅，过滤掉已不在线的用户）
                    if let Ok(db_subscriptions) = sqlx::query_scalar::<_, String>(
                        "SELECT subscription_id FROM subscriptions 
                         WHERE user_id = ? 
                         AND created_at >= DATE_SUB(NOW(), INTERVAL 24 HOUR)
                         ORDER BY created_at DESC"
                    )
                    .bind(to_user.id)
                    .fetch_all(&pool)
                    .await
                    {
                        for sub_id in &db_subscriptions {
                            subscription_service.add_subscription_id(sub_id.clone(), to_user.id);
                        }
                        ids = subscription_service.get_subscription_ids(to_user.id);
                    }
                }
                ids
            };
            
            // 判断用户是否在线
            let is_online = !subscription_ids.is_empty();
            let is_call_invite = req.message_content_type == 4;
            
            // 重要：对于通话邀请消息（message_content_type === 4），如果用户不在线，只存储到数据库，不推送
            // 因为通话邀请是实时消息，过期后没有意义，不应该在用户上线后弹出
            if is_call_invite && !is_online {
                info!(
                    to_id = %req.to_id,
                    to_open_id = %to_open_id,
                    user_db_id = to_user.id,
                    to_mqtt_id = %to_mqtt_id,
                    message_id = %message_id,
                    message_content_type = 4,
                    "语音/视频呼叫消息，用户不在线，只存储到数据库，不推送（通话邀请是实时消息，过期后无意义）"
                );
                // 只存储到数据库，不通过 MQTT 推送，也不存储到 Redis
                return Ok(Json(json!({
                    "status": "ok",
                    "message_id": message_id,
                    "stored_only": true, // 标记为仅存储，未推送
                })));
            }
            
            // 对于普通消息或在线用户的通话邀请，正常处理：
            // 1. 消息已保存到数据库（上面已完成）
            // 2. 通过 MQTT 发布消息（broker 会自动处理离线消息，使用 QoS 1 和 clean_session=false）
            // 这样即使 MQTT 推送失败或用户不在线，消息也不会丢失
            // 注意：broker 只有在客户端已经订阅过 topic 的情况下才会存储离线消息
            // 如果用户从未连接过，broker 不会存储消息，但消息已保存到数据库，用户重连后可以从数据库获取
            let topic = mqtt_user_topic(&to_mqtt_id.to_string());
            info!(
                to_id = %req.to_id, 
                user_db_id = to_user.id, 
                to_mqtt_id = %to_mqtt_id,
                has_subscription = is_online, 
                subscription_count = subscription_ids.len(), 
                %topic, 
                message_id = %message_id,
                is_call_invite = is_call_invite,
                "消息已保存到数据库，准备通过MQTT发布"
            );
            
            // 添加调试日志，确认 chat_type 是否正确设置
            info!(
                to_id = %req.to_id,
                to_mqtt_id = %to_mqtt_id,
                %topic,
                message_id = %message_id,
                chat_type = ?chat_message.chat_type,
                from_user_id = %chat_message.from_user_id,
                to_user_id = %chat_message.to_user_id,
                "准备编码并发布MQTT消息（单聊）"
            );
            
            match encode_message(&chat_message) {
                Ok(payload) => {
                    // 尝试解析 payload 以确认 chat_type 是否被正确序列化
                    if let Ok(decoded) = serde_json::from_slice::<serde_json::Value>(&payload) {
                        info!(
                            to_id = %req.to_id,
                            message_id = %message_id,
                            chat_type_in_payload = ?decoded.get("chat_type"),
                            "消息编码成功，chat_type 检查"
                        );
                    }
                    
                    info!(
                        to_id = %req.to_id,
                        to_mqtt_id = %to_mqtt_id,
                        %topic,
                        message_id = %message_id,
                        payload_len = payload.len(),
                        "准备发布MQTT消息"
                    );
                    let mqtt_publish_result = publisher.publish(&topic, payload.clone()).await;
                    
                    // 混合方案：MQTT + Redis 离线消息
                    // 1. MQTT 处理短期离线（用户曾经连接过，broker 会自动存储）
                    // 2. Redis 处理长期离线或从未连接的用户（作为备份）
                    // 重要：对于语音/视频呼叫消息（message_content_type === 4），如果用户不在线，不存储到 Redis
                    // 因为通话邀请是实时消息，过期后没有意义
                    // 注意：这里 is_online 一定是 true（因为离线用户的通话邀请已经在上面处理了）
                    let should_store_to_redis = if is_call_invite && !is_online {
                        // 语音/视频呼叫消息，用户不在线，不存储到 Redis
                        info!(
                            to_id = %req.to_id,
                            to_open_id = %to_open_id,
                            message_id = %message_id,
                            message_content_type = 4,
                            "语音/视频呼叫消息，用户不在线，不存储到 Redis（通话邀请是实时消息，过期后无意义）"
                        );
                        false
                    } else {
                        true
                    };
                    
                    if let Err(e) = mqtt_publish_result {
                        error!(
                            to_id = %req.to_id, 
                            to_mqtt_id = %to_mqtt_id,
                            %topic, 
                            message_id = %message_id,
                            error = %e, 
                            "MQTT 发布失败，将消息存储到 Redis 作为备份"
                        );
                        // MQTT 发布失败，存储到 Redis 作为备份（除非是语音/视频呼叫且用户不在线）
                        if should_store_to_redis {
                            if let Ok(payload_str) = String::from_utf8(payload.clone()) {
                                if let Err(redis_err) = redis_client.add_offline_message(&to_open_id, &payload_str).await {
                                    warn!(
                                        to_id = %req.to_id,
                                        to_open_id = %to_open_id,
                                        error = %redis_err,
                                        "Redis 离线消息存储失败（消息已保存到数据库，不会丢失）"
                                    );
                                } else {
                                    info!(
                                        to_id = %req.to_id,
                                        to_open_id = %to_open_id,
                                        message_id = %message_id,
                                        "✅ 消息已存储到 Redis（MQTT 发布失败时的备份）"
                                    );
                                }
                            }
                        }
                    } else {
                        // MQTT 发布成功
                        // 重要：对于普通消息，无论用户是否在线，都存储到 Redis 作为备份
                        // 但对于语音/视频呼叫消息，如果用户不在线，不存储（因为过期后无意义）
                        // 原因：
                        // 1. 如果用户在线但 WebSocket 连接不稳定，消息可能丢失
                        // 2. 如果用户在消息发布后才连接，MQTT broker 不会存储消息（因为订阅发生在发布之后）
                        // 3. Redis 作为统一备份，确保消息不丢失
                        if should_store_to_redis {
                            if let Ok(payload_str) = String::from_utf8(payload.clone()) {
                                if let Err(redis_err) = redis_client.add_offline_message(&to_open_id, &payload_str).await {
                                    warn!(
                                        to_id = %req.to_id,
                                        to_open_id = %to_open_id,
                                        error = %redis_err,
                                        "Redis 离线消息存储失败（MQTT 已发布，消息可能不会丢失）"
                                    );
                                } else {
                                    if is_online {
                                        info!(
                                            to_id = %req.to_id,
                                            to_mqtt_id = %to_mqtt_id,
                                            to_open_id = %to_open_id,
                                            %topic,
                                            message_id = %message_id,
                                            "✅ 消息已保存到数据库，MQTT 发布成功，Redis 已备份（用户在线，三重保障）"
                                        );
                                    } else {
                                        info!(
                                            to_id = %req.to_id,
                                            to_open_id = %to_open_id,
                                            message_id = %message_id,
                                            "✅ 消息已保存到数据库，MQTT broker 和 Redis 双重存储（用户离线，确保消息不丢失）"
                                        );
                                    }
                                }
                            }
                        } else {
                            info!(
                                to_id = %req.to_id,
                                to_open_id = %to_open_id,
                                message_id = %message_id,
                                "✅ 消息已保存到数据库，MQTT 发布成功（语音/视频呼叫消息，用户不在线，不存储到 Redis）"
                            );
                        }
                    }
                }
                Err(e) => {
                    error!(
                        message_id = %message_id,
                        error = %e, 
                        "消息编码失败"
                    );
                }
            }
            
            // 更新发送者和接收者的聊天记录
            // 注意：from_user 已经在上面获取过了，这里不需要重复获取
            
            let chat_service = ImChatService::new(pool.clone());
            let from_external_id = from_user.get_external_id();
            let to_external_id = to_user.get_external_id();
            
            // 生成统一的 chat_id（使用排序后的用户ID，确保双方使用相同的 chat_id）
            let (min_id, max_id) = if from_external_id < to_external_id {
                (&from_external_id, &to_external_id)
            } else {
                (&to_external_id, &from_external_id)
            };
            let chat_id = format!("single_{}_{}", min_id, max_id);
            
            // 为发送者更新或创建聊天记录（发送者视角：to_id 是接收者）
            // 注意：即使 get_or_create_chat 失败，消息也已经保存并发送，不会影响消息接收
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                1, // chat_type: 1 = 单聊
                from_external_id.clone(),
                to_external_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, from_id = %from_external_id, to_id = %to_external_id, error = ?e, "创建或获取发送者聊天记录失败（消息已保存并发送，不影响消息接收）");
            } else {
                // 更新聊天记录的 sequence 和 update_time（同时指定 chat_id、owner_id 和 chat_type，确保类型正确）
                if let Err(e) = sqlx::query(
                    "UPDATE im_chat 
                     SET sequence = ?, update_time = ?, version = version + 1 
                     WHERE chat_id = ? AND owner_id = ? AND chat_type = 1"
                )
                .bind(now)
                .bind(now)
                .bind(&chat_id)
                .bind(&from_external_id)
                .execute(&pool)
                .await {
                    warn!(error = %e, "更新发送者聊天记录失败（消息已保存并发送，不影响消息接收）");
                }
            }
            
            // 为接收者更新或创建聊天记录（接收者视角：to_id 是发送者）
            // 注意：这里使用相同的 chat_id，但 owner_id 和 to_id 不同
            // 注意：即使 get_or_create_chat 失败，消息也已经保存并发送，不会影响消息接收
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                1, // chat_type: 1 = 单聊
                to_external_id.clone(),
                from_external_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, from_id = %to_external_id, to_id = %from_external_id, error = ?e, "创建或获取接收者聊天记录失败（消息已保存并发送，不影响消息接收）");
            } else {
                // 更新聊天记录的 sequence 和 update_time（同时指定 chat_id、owner_id 和 chat_type，确保类型正确）
                if let Err(e) = sqlx::query(
                    "UPDATE im_chat 
                     SET sequence = ?, update_time = ?, version = version + 1 
                     WHERE chat_id = ? AND owner_id = ? AND chat_type = 1"
                )
                .bind(now)
                .bind(now)
                .bind(&chat_id)
                .bind(&to_external_id)
                .execute(&pool)
                .await {
                    warn!(error = %e, "更新接收者聊天记录失败（消息已保存并发送，不影响消息接收）");
                }
            }
            
            Ok(Json(serde_json::json!({"status": "ok"})))
        },
        Err(e) => {
            error!("保存单聊消息失败: {:?}, 请求: from_id={}, to_id={}, message_body={}", 
                e, req.from_id, req.to_id, req.message_body);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, format!("发送消息失败: {:?}", e))),
            ))
        },
    }
}

pub async fn get_single_messages(
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(user_id): Extension<u64>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    let user_service = UserService::new(pool);
    
    // 将数据库ID转换为 open_id（因为数据库中的 from_id 和 to_id 都是 open_id）
    let from_open_id = match user_service.get_by_id(user_id).await {
        Ok(user) => user.get_external_id(),
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::NotFound, "当前用户不存在")),
            ));
        }
    };
    
    let to_id = params.get("to_id").cloned().unwrap_or_default();
    let since_sequence = params.get("since_sequence").and_then(|s| s.parse::<i64>().ok());
    let limit = params.get("limit").and_then(|s| s.parse::<i32>().ok()).unwrap_or(100);
    
    match service.get_single_messages(&from_open_id, &to_id, since_sequence, limit).await {
        Ok(messages) => Ok(Json(serde_json::json!({"messages": messages}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取消息失败")),
        )),
    }
}

pub async fn mark_single_message_read(
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(user_id): Extension<u64>,
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    let service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    let user_service = UserService::new(pool);
    
    // 将数据库ID转换为 open_id（因为数据库中的 to_id 是 open_id）
    let to_open_id = match user_service.get_by_id(user_id).await {
        Ok(user) => user.get_external_id(),
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::NotFound, "当前用户不存在")),
            ));
        }
    };
    
    match service.mark_single_message_read(&message_id, &to_open_id).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "标记消息已读失败")),
        )),
    }
}

pub async fn send_group_message(
    State((publisher, subscription_service)): State<(MqttPublisher, Arc<SubscriptionService>)>,
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Json(req): Json<SendGroupMessageRequest>,
) -> impl IntoResponse {
    let service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    let group_service = ImGroupService::new(pool.clone());
    let user_service = UserService::new(pool.clone());
    let chat_service = ImChatService::new(pool.clone());
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    
    // 先获取发送者的 open_id，确保统一使用 open_id
    let from_user = match user_service.get_by_open_id(&req.from_id).await {
        Ok(user) => user,
        Err(_) => {
            // 作为用户名查找
            match user_service.get_by_name(&req.from_id).await {
                Ok(user) => user,
                Err(_) => {
                    warn!(from_id = %req.from_id, "无法找到发送者用户");
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::new(ErrorCode::NotFound, "发送者用户不存在")),
                    ));
                }
            }
        }
    };
    
    // 统一使用 open_id 作为消息的 from_id
    let from_open_id = from_user.get_external_id();
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    
    let message_id = Uuid::new_v4().to_string();
    
    // 统一 group_id 格式：确保有 group_ 前缀
    let normalized_group_id = if req.group_id.starts_with("group_") {
        req.group_id.clone()
    } else {
        format!("group_{}", req.group_id)
    };
    
    info!(
        original_group_id = %req.group_id,
        normalized_group_id = %normalized_group_id,
        from_id = %from_open_id,
        message_id = %message_id,
        "开始发送群消息"
    );
    
    // 先检查群组是否存在且未解散
    match group_service.get_group(&normalized_group_id).await {
        Ok(group) => {
            if group.del_flag == 0 {
                warn!(
                    original_group_id = %req.group_id,
                    normalized_group_id = %normalized_group_id,
                    "群组已解散，无法发送消息"
                );
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(ErrorCode::InvalidInput, "群组已解散，无法发送消息")),
                ));
            }
        },
        Err(e) => {
            // 如果群组不存在，可能是2人聊天，继续处理
            // 但如果是3人及以上的群组，应该返回错误
            warn!(
                original_group_id = %req.group_id,
                normalized_group_id = %normalized_group_id,
                error = ?e,
                "群组不存在或已解散"
            );
            // 对于群组不存在的情况，我们仍然尝试获取成员
            // 如果成员数为0，说明群组确实不存在或已解散
        }
    }
    
    // 先获取群组的所有成员
    let members = match group_service.get_group_members(&normalized_group_id).await {
        Ok(members) => {
            info!(
                original_group_id = %req.group_id,
                normalized_group_id = %normalized_group_id,
                member_count = members.len(),
                "获取群成员成功"
            );
            members
        },
        Err(e) => {
            error!(
                original_group_id = %req.group_id,
                normalized_group_id = %normalized_group_id,
                error = ?e,
                "获取群成员失败"
            );
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取群成员失败")),
            ));
        }
    };
    
    // 根据 chat_type 决定使用单聊还是群聊逻辑，而不是根据成员数
    // 重要：以 chat_type 为主判断，人数只能作为辅助
    // 原因：有可能开始拉群人数超过2个人，后面群主把人员移除群聊，这个群就剩下他一个人
    // 如果以人数判断，就会有bug
    // 构建 chat_id：从 normalized_group_id 中提取原始 group_id（去掉 group_ 前缀）
    let original_group_id = normalized_group_id.trim_start_matches("group_").to_string();
    let chat_id = format!("group_{}", original_group_id);
    let chat_type = match chat_service.get_or_create_chat(
        chat_id.clone(),
        2, // 默认群聊类型
        from_open_id.clone(),
        normalized_group_id.clone(),
    ).await {
        Ok(chat) => {
            info!(
                chat_id = %chat_id,
                chat_type = chat.chat_type,
                "从 im_chat 表获取 chat_type 成功"
            );
            chat.chat_type
        },
        Err(e) => {
            // 如果查询失败，默认使用群聊类型（chat_type = 2）
            warn!(
                chat_id = %chat_id,
                error = ?e,
                "查询 chat_type 失败，默认使用群聊类型（chat_type = 2）"
            );
            2
        }
    };
    
    let member_count = members.len();
    let is_single_chat = chat_type == 1;
    
    info!(
        original_group_id = %req.group_id,
        normalized_group_id = %normalized_group_id,
        chat_type = chat_type,
        member_count = member_count,
        is_single_chat = is_single_chat,
        "根据 chat_type 决定聊天类型：chat_type=1为单聊，chat_type=2为群聊（人数仅作为辅助）"
    );
    
    // 解析extra字段获取文件信息
    let mut file_url = None;
    let mut file_name = None;
    let mut file_type = None;
    
    if let Some(extra_str) = &req.extra {
        if let Ok(extra_json) = serde_json::from_str::<serde_json::Value>(extra_str) {
            file_url = extra_json.get("file_url").and_then(|v| v.as_str()).map(|s| s.to_string());
            file_name = extra_json.get("file_name").and_then(|v| v.as_str()).map(|s| s.to_string());
            file_type = extra_json.get("file_type").and_then(|v| v.as_str()).map(|s| s.to_string());
        }
    }
    
    // 根据 chat_type 决定保存到哪个表：chat_type=1保存到单聊表，chat_type=2保存到群聊表
    if is_single_chat {
        // chat_type=1（单聊）：保存到单聊表
        // 找到接收者（除了发送者之外的成员）
        let mut receiver_user_option = None;
        for member in &members {
            let member_user = match user_service.get_by_open_id(&member.member_id).await {
                Ok(user) => user,
                Err(_) => {
                    match user_service.get_by_name(&member.member_id).await {
                        Ok(user) => user,
                        Err(_) => continue,
                    }
                }
            };
            let member_open_id = member_user.get_external_id();
            let member_db_id = member_user.id;
            if member_open_id != from_open_id && member_db_id != from_user.id {
                receiver_user_option = Some(member_user);
                break;
            }
        }
        
        if let Some(receiver_user) = receiver_user_option {
            let receiver_open_id = receiver_user.get_external_id();
            
            // 保存到单聊表（双向保存：from->to 和 to->from）
            let single_message = ImSingleMessage {
                message_id: message_id.clone(),
                from_id: from_open_id.clone(),
                to_id: receiver_open_id.clone(),
                message_body: req.message_body.clone(),
                message_time: now,
                message_content_type: req.message_content_type,
                read_status: 0,
                extra: req.extra.clone(),
                del_flag: 1,
                sequence: now,
                message_random: Some(Uuid::new_v4().to_string()),
                create_time: Some(now),
                update_time: Some(now),
                version: Some(1),
                reply_to: req.reply_to.clone(),
                to_type: Some("User".to_string()),
                file_url: None,
                file_name: None,
                file_type: None,
            };
            
            match service.save_single_message(single_message).await {
                Ok(_) => {
                    info!(group_id = %req.group_id, message_id = %message_id, chat_type = 1, "单聊消息已保存到单聊表");
                },
                Err(e) => {
                    error!(group_id = %req.group_id, error = ?e, chat_type = 1, "保存单聊消息到单聊表失败");
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(e, "保存消息失败")),
                    ));
                }
            }
        } else {
            error!(group_id = %req.group_id, chat_type = 1, "无法找到接收者，无法保存单聊消息");
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::NotFound, "无法找到接收者")),
            ));
        }
    } else {
        // chat_type=2（群聊）：保存到群聊表
        let group_message = ImGroupMessage {
            message_id: message_id.clone(),
            group_id: normalized_group_id.clone(),
            from_id: from_open_id.clone(), // 使用 open_id
            message_body: req.message_body.clone(),
            message_time: now,
            message_content_type: req.message_content_type,
            extra: req.extra.clone(),
            del_flag: 1,
            sequence: Some(now),
            message_random: Some(Uuid::new_v4().to_string()),
            create_time: now,
            update_time: Some(now),
            version: Some(1),
            reply_to: req.reply_to.clone(),
        };
        
        match service.save_group_message(group_message).await {
            Ok(_) => {
                info!(group_id = %req.group_id, message_id = %message_id, chat_type = 2, "群聊消息已保存到群聊表");
            },
            Err(e) => {
                error!(group_id = %req.group_id, error = ?e, chat_type = 2, "保存群聊消息到群聊表失败");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(e, "保存消息失败")),
                ));
            }
        }
    }
    
    // 消息保存成功后，继续处理推送和聊天记录更新
    // 去重：使用 HashSet 确保每个 member_id 只处理一次
    // 这样可以避免数据库中有重复记录时导致重复发送消息
    use std::collections::HashSet;
    let mut processed_member_ids = HashSet::new();
    let mut skipped_sender_count = 0;
    let mut skipped_duplicate_count = 0;
    
    // 获取发送者的 open_id 和内部ID，用于比较
    let from_user_open_id = from_open_id.clone();
    let from_user_db_id = from_user.id;
    
    // 为每个群成员（除了发送者）推送消息
    for member in &members {
        let member_id_str = &member.member_id;
        
        // 获取成员用户信息（需要先获取才能比较）
        let member_user = match user_service.get_by_open_id(member_id_str).await {
            Ok(user) => user,
            Err(_) => {
                match user_service.get_by_name(member_id_str).await {
                    Ok(user) => user,
                    Err(_) => {
                        warn!(member_id = %member_id_str, "无法找到群成员用户，跳过推送");
                        continue;
                    }
                }
            }
        };
        
        // 跳过发送者自己：比较 open_id 或数据库ID
        // 因为 member_id 可能是用户名、open_id 或 snowflake_id，需要统一比较
        let member_open_id = member_user.get_external_id();
        let member_db_id = member_user.id;
        
        if member_open_id == from_user_open_id || member_db_id == from_user_db_id {
            skipped_sender_count += 1;
            info!(group_id = %req.group_id, member_id = %member_id_str, member_open_id = %member_open_id, from_open_id = %from_user_open_id, "跳过发送者自己");
            continue;
        }
        
        // 如果已经处理过这个成员，跳过（去重）
        // 使用 open_id 作为唯一标识，因为它是稳定的外部标识符
        if !processed_member_ids.insert(member_open_id.clone()) {
            skipped_duplicate_count += 1;
            warn!(group_id = %req.group_id, member_id = %member_id_str, member_open_id = %member_open_id, "检测到重复的群成员记录，跳过重复发送");
            continue;
        }
        
        // 获取成员的MQTT ID
        let member_mqtt_id = member_user.get_mqtt_id();
        
        // 根据 chat_type 决定聊天类型和接收者ID（以 chat_type 为主，而不是成员数）
        // chat_type=1（单聊），使用对方的 open_id 作为 to_user_id
        // chat_type=2（群聊），使用 group_id 作为 to_user_id
        let (chat_type_for_message, to_user_id) = if is_single_chat {
            // 单聊：使用对方的 open_id
            (Some(1), member_open_id.clone())
        } else {
            // 群聊：使用 normalized_group_id
            (Some(2), normalized_group_id.clone())
        };
        
        // 构建消息格式
        let chat_message = ChatMessage {
            message_id: message_id.clone(),
            from_user_id: from_user_open_id.clone(), // 使用 open_id
            to_user_id: to_user_id.clone(), // 单聊使用对方 open_id，群聊使用 group_id
            message: req.message_body.clone(),
            timestamp_ms: now,
            file_url: file_url.clone(),
            file_name: file_name.clone(),
            file_type: file_type.clone(),
            chat_type: chat_type_for_message, // 根据 chat_type 决定：chat_type=1（单聊），chat_type=2（群聊）
        };
        
        // 从数据库查询订阅ID并同步到内存（如果内存中没有）
        let subscription_ids = {
            let mut ids = subscription_service.get_subscription_ids(member_user.id);
            if ids.is_empty() {
                // 如果内存中没有，从数据库查询（只查询最近24小时内创建的订阅，过滤掉已不在线的用户）
                if let Ok(db_subscriptions) = sqlx::query_scalar::<_, String>(
                    "SELECT subscription_id FROM subscriptions 
                     WHERE user_id = ? 
                     AND created_at >= DATE_SUB(NOW(), INTERVAL 24 HOUR)
                     ORDER BY created_at DESC"
                )
                .bind(member_user.id)
                .fetch_all(&pool)
                .await
                {
                    for sub_id in &db_subscriptions {
                        subscription_service.add_subscription_id(sub_id.clone(), member_user.id);
                    }
                    ids = subscription_service.get_subscription_ids(member_user.id);
                }
            }
            ids
        };
        
        // 通过 MQTT 发布消息给群成员
        // 注意：broker 只有在客户端已经订阅过 topic 的情况下才会存储离线消息
        // 如果用户从未连接过，broker 不会存储消息，但消息已保存到数据库，用户可以通过其他方式获取
        let topic = mqtt_user_topic(&member_mqtt_id.to_string());
        let is_online = !subscription_ids.is_empty();
        info!(group_id = %req.group_id, member_id = %member_id_str, is_online = is_online, subscription_count = subscription_ids.len(), %topic, "通过MQTT发布群消息（broker会自动处理离线消息，前提是用户曾经订阅过topic）");
        
        // 添加调试日志，确认消息的 chat_type 是否正确设置
        let chat_type_str = if is_single_chat { "单聊" } else { "群聊" };
        info!(
            group_id = %req.group_id,
            member_id = %member_id_str,
            member_count = member_count,
            %topic,
            message_id = %message_id,
            chat_type = ?chat_message.chat_type,
            from_user_id = %chat_message.from_user_id,
            to_user_id = %chat_message.to_user_id,
            "准备编码并发布MQTT消息（{}）",
            chat_type_str
        );
        
        match encode_message(&chat_message) {
            Ok(payload) => {
                // 尝试解析 payload 以确认 chat_type 是否被正确序列化
                if let Ok(decoded) = serde_json::from_slice::<serde_json::Value>(&payload) {
                    info!(
                        group_id = %req.group_id,
                        member_id = %member_id_str,
                        message_id = %message_id,
                        chat_type_in_payload = ?decoded.get("chat_type"),
                        "群组消息编码成功，chat_type 检查"
                    );
                }
                
                // 混合方案：MQTT + Redis 离线消息
                // 1. MQTT 处理短期离线（用户曾经连接过，broker 会自动存储）
                // 2. Redis 处理长期离线或从未连接的用户（作为备份）
                // 先转换为 String，以便在多个地方使用
                let payload_str_result = String::from_utf8(payload.clone());
                
                // 检查 payload 转换是否成功
                if let Err(e) = &payload_str_result {
                    error!(
                        group_id = %req.group_id,
                        member_id = %member_id_str,
                        member_open_id = %member_open_id,
                        message_id = %message_id,
                        error = %e,
                        "⚠️ 群组消息 payload 转换为 String 失败，无法存储到 Redis"
                    );
                }
                
                if let Err(e) = publisher.publish(&topic, payload).await {
                    error!(group_id = %req.group_id, member_id = %member_id_str, %topic, error = %e, message_id = %message_id, chat_type = ?chat_type, "消息MQTT发布失败，将消息存储到 Redis 作为备份");
                    // MQTT 发布失败，存储到 Redis 作为备份
                    match payload_str_result {
                        Ok(payload_str) => {
                            if let Err(redis_err) = redis_client.add_offline_message(&member_open_id, &payload_str).await {
                                warn!(
                                    group_id = %req.group_id,
                                    member_id = %member_id_str,
                                    member_open_id = %member_open_id,
                                    error = %redis_err,
                                    "Redis 离线消息存储失败（消息已保存到数据库，不会丢失）"
                                );
                            } else {
                                info!(
                                    group_id = %req.group_id,
                                    member_id = %member_id_str,
                                    member_open_id = %member_open_id,
                                    message_id = %message_id,
                                    chat_type = ?chat_type,
                                    "✅ 消息已存储到 Redis（MQTT 发布失败时的备份）"
                                );
                            }
                        }
                        Err(e) => {
                            error!(
                                group_id = %req.group_id,
                                member_id = %member_id_str,
                                member_open_id = %member_open_id,
                                message_id = %message_id,
                                error = %e,
                                "⚠️ 无法将消息存储到 Redis（payload 转换失败）"
                            );
                        }
                    }
                } else {
                    // MQTT 发布成功
                    // 重要：无论用户是否在线，都存储到 Redis 作为备份
                    // 原因：
                    // 1. 如果用户在线但 WebSocket 连接不稳定，消息可能丢失
                    // 2. 如果用户在消息发布后才连接，MQTT broker 不会存储消息（因为订阅发生在发布之后）
                    // 3. Redis 作为统一备份，确保消息不丢失
                    match payload_str_result {
                        Ok(payload_str) => {
                            info!(
                                group_id = %req.group_id,
                                member_id = %member_id_str,
                                member_open_id = %member_open_id,
                                message_id = %message_id,
                                chat_type = ?chat_type,
                                payload_length = payload_str.len(),
                                "准备存储群组消息到 Redis"
                            );
                            
                            if let Err(redis_err) = redis_client.add_offline_message(&member_open_id, &payload_str).await {
                                warn!(
                                    group_id = %req.group_id,
                                    member_id = %member_id_str,
                                    member_open_id = %member_open_id,
                                    message_id = %message_id,
                                    chat_type = ?chat_type,
                                    error = %redis_err,
                                    "❌ Redis 离线消息存储失败（MQTT 已发布，消息可能不会丢失）"
                                );
                            } else {
                                if is_online {
                                    info!(
                                        group_id = %req.group_id,
                                        member_id = %member_id_str,
                                        member_open_id = %member_open_id,
                                        %topic,
                                        message_id = %message_id,
                                        chat_type = ?chat_type,
                                        "✅ 消息已保存到数据库，MQTT 发布成功，Redis 已备份（用户在线，三重保障）"
                                    );
                                } else {
                                    info!(
                                        group_id = %req.group_id,
                                        member_id = %member_id_str,
                                        member_open_id = %member_open_id,
                                        message_id = %message_id,
                                        chat_type = ?chat_type,
                                        "✅ 消息已保存到数据库，MQTT broker 和 Redis 双重存储（用户离线，确保消息不丢失）"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            error!(
                                group_id = %req.group_id,
                                member_id = %member_id_str,
                                member_open_id = %member_open_id,
                                message_id = %message_id,
                                error = %e,
                                "⚠️ 无法将消息存储到 Redis（payload 转换失败），MQTT 已发布"
                            );
                        }
                    }
                }
            }
            Err(e) => {
                error!(member_id = %member_id_str, error = %e, "群消息编码失败");
            }
        }
    }
    
    info!(
        group_id = %req.group_id, 
        message_id = %message_id,
        total_members = members.len(),
        member_count = member_count,
        is_single_chat = is_single_chat,
        processed_count = processed_member_ids.len(),
        skipped_sender = skipped_sender_count,
        skipped_duplicate = skipped_duplicate_count,
        "消息发送完成（{}）",
        if is_single_chat { "单聊" } else { "群聊" }
    );
    
    // 更新聊天记录（为所有成员更新，包括发送者）
    let chat_service = ImChatService::new(pool.clone());
    let from_external_id = from_user.get_external_id();
    
    // 如果是单聊，需要为发送者和接收者都创建聊天记录
    if is_single_chat {
        // 找到接收者
        let mut receiver_user_option = None;
        for member in &members {
            let member_user = match user_service.get_by_open_id(&member.member_id).await {
                Ok(user) => user,
                Err(_) => {
                    match user_service.get_by_name(&member.member_id).await {
                        Ok(user) => user,
                        Err(_) => continue,
                    }
                }
            };
            let member_open_id = member_user.get_external_id();
            let member_db_id = member_user.id;
            if member_open_id != from_external_id && member_db_id != from_user_db_id {
                receiver_user_option = Some(member_user);
                break;
            }
        }
        
        if let Some(receiver_user) = receiver_user_option {
            let receiver_external_id = receiver_user.get_external_id();
            
            // 生成统一的 chat_id（使用排序后的用户ID）
            let (min_id, max_id) = if from_external_id < receiver_external_id {
                (&from_external_id, &receiver_external_id)
            } else {
                (&receiver_external_id, &from_external_id)
            };
            let chat_id = format!("single_{}_{}", min_id, max_id);
            
            // 为发送者创建聊天记录
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                1, // chat_type: 1 = 单聊
                from_external_id.clone(),
                receiver_external_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, member_id = %from_external_id, error = ?e, "创建或获取发送者聊天记录失败");
            } else {
                // 更新聊天记录的 sequence 和 update_time
                if let Err(e) = sqlx::query(
                    "UPDATE im_chat 
                     SET sequence = ?, update_time = ?, version = version + 1 
                     WHERE chat_id = ? AND owner_id = ? AND chat_type = 1"
                )
                .bind(now)
                .bind(now)
                .bind(&chat_id)
                .bind(&from_external_id)
                .execute(&pool)
                .await {
                    warn!(error = %e, "更新发送者聊天记录失败");
                }
            }
            
            // 为接收者创建聊天记录
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                1, // chat_type: 1 = 单聊
                receiver_external_id.clone(),
                from_external_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, member_id = %receiver_external_id, error = ?e, "创建或获取接收者聊天记录失败");
            } else {
                // 更新聊天记录的 sequence 和 update_time
                if let Err(e) = sqlx::query(
                    "UPDATE im_chat 
                     SET sequence = ?, update_time = ?, version = version + 1 
                     WHERE chat_id = ? AND owner_id = ? AND chat_type = 1"
                )
                .bind(now)
                .bind(now)
                .bind(&chat_id)
                .bind(&receiver_external_id)
                .execute(&pool)
                .await {
                    warn!(error = %e, "更新接收者聊天记录失败");
                }
            }
        }
    }
    
    // 为群成员（除了发送者）更新聊天记录（仅群聊）
    for member in &members {
        let member_user = match user_service.get_by_open_id(&member.member_id).await {
            Ok(user) => user,
            Err(_) => {
                match user_service.get_by_name(&member.member_id).await {
                    Ok(user) => user,
                    Err(_) => {
                        warn!(member_id = %member.member_id, "无法找到群成员用户，跳过更新聊天记录");
                        continue;
                    }
                }
            }
        };
        
        let member_external_id = member_user.get_external_id();
        
        // 只处理群聊的聊天记录（单聊已经在上面处理了）
        if !is_single_chat {
            // 群聊：使用 group_ 前缀
            let chat_id = format!("group_{}", req.group_id);
            let to_id = req.group_id.clone();
            
            // 为每个成员更新或创建群聊记录
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                2, // chat_type: 2 = 群聊
                member_external_id.clone(),
                to_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, member_id = %member_external_id, error = ?e, "创建或获取群聊记录失败");
            } else {
                // 更新聊天记录的 sequence 和 update_time
                if let Err(e) = sqlx::query(
                    "UPDATE im_chat 
                     SET sequence = ?, update_time = ?, version = version + 1 
                     WHERE chat_id = ? AND owner_id = ? AND chat_type = 2"
                )
                .bind(now)
                .bind(now)
                .bind(&chat_id)
                .bind(&member_external_id)
                .execute(&pool)
                .await {
                    warn!(error = %e, "更新群组聊天记录失败");
                }
            }
        }
    }
    
    Ok(Json(serde_json::json!({"status": "ok"})))
}

pub async fn get_group_messages(
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(user_id): Extension<u64>,
    Path(group_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    let group_service = ImGroupService::new(pool.clone());
    let user_service = UserService::new(pool.clone());
    
    // 获取当前用户信息
    let current_user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    // 获取群组成员数，决定查询哪个表
    let members = match group_service.get_group_members(&group_id).await {
        Ok(members) => members,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取群成员失败")),
            ));
        }
    };
    
    let member_count = members.len();
    let is_single_chat = member_count == 2;
    
    let since_sequence = params.get("since_sequence").and_then(|s| s.parse::<i64>().ok());
    let limit = params.get("limit").and_then(|s| s.parse::<i32>().ok()).unwrap_or(100);
    
    // 根据成员数决定查询哪个表
    if is_single_chat {
        // 2人聊天：从单聊表查询
        // 找到对方的 open_id
        let current_user_open_id = current_user.get_external_id();
        let mut other_user_open_id = None;
        
        for member in &members {
            let member_user = match user_service.get_by_open_id(&member.member_id).await {
                Ok(user) => user,
                Err(_) => {
                    match user_service.get_by_name(&member.member_id).await {
                        Ok(user) => user,
                        Err(_) => continue,
                    }
                }
            };
            let member_open_id = member_user.get_external_id();
            if member_open_id != current_user_open_id {
                other_user_open_id = Some(member_open_id);
                break;
            }
        }
        
        if let Some(other_id) = other_user_open_id {
            match service.get_single_messages(&current_user_open_id, &other_id, since_sequence, limit).await {
                Ok(messages) => {
                    // 将单聊消息转换为统一的格式返回
                    let converted_messages: Vec<serde_json::Value> = messages.iter().map(|msg| {
                        serde_json::json!({
                            "message_id": msg.message_id,
                            "group_id": group_id, // 保留 group_id 以便前端识别
                            "from_id": msg.from_id,
                            "message_body": msg.message_body,
                            "message_time": msg.message_time,
                            "message_content_type": msg.message_content_type,
                            "extra": msg.extra,
                            "del_flag": msg.del_flag,
                            "sequence": msg.sequence,
                            "message_random": msg.message_random,
                            "create_time": msg.create_time,
                            "update_time": msg.update_time,
                            "version": msg.version,
                            "reply_to": msg.reply_to,
                        })
                    }).collect();
                    Ok(Json(serde_json::json!({"messages": converted_messages})))
                },
                Err(e) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(e, "获取单聊消息失败")),
                )),
            }
        } else {
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::NotFound, "无法找到对方用户")),
            ))
        }
    } else {
        // 3人及以上：从群聊表查询
        match service.get_group_messages(&group_id, since_sequence, limit).await {
            Ok(messages) => Ok(Json(serde_json::json!({"messages": messages}))),
            Err(e) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取群消息失败")),
            )),
        }
    }
}

pub async fn mark_group_message_read(
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(user_id): Extension<u64>,
    Path((group_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            warn!("获取当前用户失败: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let to_id = user.get_external_id();
    
    // 获取群组成员数，决定使用哪个表的已读标记
    let members = match group_service.get_group_members(&group_id).await {
        Ok(members) => members,
        Err(e) => {
            warn!("获取群成员失败: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取群成员失败")),
            ));
        }
    };
    
    let member_count = members.len();
    let is_single_chat = member_count == 2;
    
    // 根据成员数决定使用哪个表的已读标记
    if is_single_chat {
        // 2人聊天：使用单聊表的 read_status 字段
        match service.mark_single_message_read(&message_id, &to_id).await {
            Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
            Err(e) => Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, "标记单聊消息已读失败")),
            )),
        }
    } else {
        // 3人及以上：使用群聊消息状态表
        match service.mark_group_message_read(&group_id, &message_id, &to_id).await {
            Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
            Err(e) => Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, "标记群消息已读失败")),
            )),
        }
    }
}

pub async fn get_group_message_status(
    Extension(pool): Extension<MySqlPool>,
    Extension(_user_id): Extension<u64>,
    Path((group_id, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    let service = ImMessageService::new(pool);
    
    match service.get_group_message_status(&group_id, &message_id).await {
        Ok(statuses) => Ok(Json(serde_json::json!({"statuses": statuses}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取群消息状态失败")),
        )),
    }
}

pub async fn get_user_group_message_status(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(group_id): Path<String>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImMessageService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            warn!("获取当前用户失败: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let to_id = user.get_external_id();
    let limit = params.get("limit").and_then(|s| s.parse::<i32>().ok());
    
    match service.get_user_group_message_status(&group_id, &to_id, limit).await {
        Ok(statuses) => Ok(Json(serde_json::json!({"statuses": statuses}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取用户群消息状态失败")),
        )),
    }
}

