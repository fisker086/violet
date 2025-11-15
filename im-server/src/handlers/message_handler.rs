use axum::{extract::{State, Extension}, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use std::sync::Arc;
use uuid::Uuid;
use sqlx::MySqlPool;
use im_share::{
    SendRequest, Target, ChatMessage, mqtt_user_topic, encode_message, get_group_members,
};
use crate::{
    error::{ErrorCode, ErrorResponse},
    mqtt::MqttPublisher,
    service::{SubscriptionService, UserService, ImMessageService},
    model::ImSingleMessage,
};

pub async fn send_message(
    State((publisher, subscription_service)): State<(MqttPublisher, Arc<SubscriptionService>)>,
    Extension(pool): Extension<MySqlPool>,
    Json(req): Json<SendRequest>,
) -> impl IntoResponse {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    // 确定接收者用户 ID（使用 open_id 的数字形式）
    let mut recipient_user_ids: Vec<u64> = match &req.target {
        Target::User(uid_or_email) => {
            let user_service = UserService::new(pool.clone());
            // 优先尝试作为 open_id 查询
            match user_service.get_by_open_id(uid_or_email).await {
                Ok(user) => {
                    vec![user.get_mqtt_id()]
                },
                Err(_) => {
                    // 如果 open_id 查询失败，尝试通过邮箱查询
                    if uid_or_email.contains('@') {
                        match user_service.get_by_email(uid_or_email).await {
                            Ok(user) => vec![user.get_mqtt_id()],
                            Err(_) => {
                                tracing::warn!(%uid_or_email, "无法找到用户（邮箱不存在）");
                                vec![]
                            }
                        }
                    } else {
                        // 尝试作为用户名查找
                        match user_service.get_by_name(uid_or_email).await {
                            Ok(user) => vec![user.get_mqtt_id()],
                            Err(_) => {
                                tracing::warn!(%uid_or_email, "无法找到用户（用户名不存在）");
                                vec![]
                            }
                        }
                    }
                }
            }
        },
        Target::Group(gid) => {
            // 群组逻辑暂时简化，实际应该从数据库获取群组成员
            get_group_members(gid)
                .into_iter()
                .filter_map(|uid| uid.parse().ok())
                .filter(|id| id != &req.from_user_id.parse().unwrap_or(0))
                .collect()
        },
    };
    recipient_user_ids.sort_unstable();
    recipient_user_ids.dedup();

    // 如果没有接收者，直接返回成功
    if recipient_user_ids.is_empty() {
        return Ok(Json(json!({"status": "ok"})));
    }

    // 为每个接收者用户找到所有订阅 ID，并发送消息
    for to_user_mqtt_id in recipient_user_ids {
        // 注意：这里 to_user_mqtt_id 是 open_id 的数字形式（用于MQTT）
        // subscription_service 使用的是数据库 id，需要根据 open_id 查找数据库 id
        let user_service = UserService::new(pool.clone());
        // 将数字转换为字符串，作为 open_id 查询
        let open_id = to_user_mqtt_id.to_string();
        let to_user = match user_service.get_by_open_id(&open_id).await {
            Ok(user) => user,
            Err(_) => {
                tracing::warn!(open_id = %open_id, "无法找到用户");
                continue;
            }
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
        
        // 无论用户是否在线，都要保存消息到数据库
        // 如果用户在线，通过 MQTT 实时推送；如果离线，用户重连后可以从数据库获取

        let message = ChatMessage {
            message_id: Uuid::new_v4().to_string(),
            from_user_id: req.from_user_id.clone(),
            to_user_id: to_user.get_external_id(), // 使用 open_id
            message: req.message.clone(),
            timestamp_ms: ts,
            file_url: req.file_url.clone(),
            file_name: req.file_name.clone(),
            file_type: req.file_type.clone(),
            chat_type: Some(1), // 1 = 单聊
        };

        // 正确处理编码错误
        let payload = match encode_message(&message) {
            Ok(p) => p,
            Err(_) => {
                tracing::error!("消息编码失败: {:?}", message);
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(
                        ErrorCode::InvalidInput,
                        "消息编码失败",
                    )),
                ));
            }
        };

        // 如果用户在线，通过 MQTT 实时推送
        if !subscription_ids.is_empty() {
            // 发布到用户的 MQTT topic（基于雪花ID）
            let topic = mqtt_user_topic(&to_user_mqtt_id.to_string());

            if let Err(e) = publisher.publish(&topic, payload.clone()).await {
                tracing::error!(user_id = %to_user_mqtt_id, %topic, error = %e, "MQTT 发布失败");
                // 不返回错误，继续保存到数据库
            }
        } else {
            tracing::info!(user_id = %to_user_mqtt_id, "用户离线，消息将保存到数据库，等待用户重连后获取");
        }

        // 无论用户是否在线，都要保存消息到数据库
        // 需要获取发送者的数据库ID（用于数据库外键）
        let from_user_service = UserService::new(pool.clone());
        // 解析发送者ID（优先使用 open_id）
        let from_user = match from_user_service.get_by_open_id(&req.from_user_id).await {
            Ok(user) => user,
            Err(_) => {
                // 尝试作为用户名查询
                match from_user_service.get_by_name(&req.from_user_id).await {
                    Ok(user) => user,
                    Err(_) => {
                        tracing::warn!(from_user_id = %req.from_user_id, "无法找到发送者用户");
                        continue; // 跳过保存
                    }
                }
            }
        };

        // 保存消息到数据库（使用 im_single_message 表）
        let to_type_str = match req.target {
            Target::User(_) => "User",
            Target::Group(_) => "Group",
        };

        let im_message_service = ImMessageService::new(pool.clone());
        let im_single_message = ImSingleMessage {
            message_id: message.message_id.clone(),
            from_id: from_user.get_external_id(), // 使用 open_id
            to_id: to_user.get_external_id(), // 使用 open_id
            message_body: message.message.clone(),
            message_time: message.timestamp_ms,
            message_content_type: 1, // 默认文本消息，可以根据 file_url 判断是否为文件
            read_status: 0, // 默认未读
            extra: None,
            del_flag: 1, // 未删除
            sequence: message.timestamp_ms, // 使用时间戳作为序列号
            message_random: Some(Uuid::new_v4().to_string()),
            create_time: Some(message.timestamp_ms),
            update_time: Some(message.timestamp_ms),
            version: Some(1),
            reply_to: None,
            to_type: Some(to_type_str.to_string()),
            file_url: message.file_url.clone(),
            file_name: message.file_name.clone(),
            file_type: message.file_type.clone(),
        };

        if let Err(e) = im_message_service.save_single_message(im_single_message).await {
            tracing::error!(error = ?e, "保存消息到数据库失败");
            // 不返回错误，因为消息已经通过 MQTT 发送成功（如果用户在线）
        }
    }

    Ok(Json(json!({"status": "ok"})))
}

// 获取离线消息
pub async fn get_messages(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<crate::middleware::auth::UserIdentity>,
    axum::extract::Query(params): axum::extract::Query<std::collections::HashMap<String, String>>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    // 使用 open_id 查询用户
    let current_user = match user_service.get_by_open_id(&user_identity.open_id).await {
        Ok(user) => user,
        Err(ErrorCode::NotFound) => {
            // 如果通过 open_id 找不到，尝试通过数据库 id（向后兼容）
            match user_service.get_by_id(user_identity.db_id).await {
                Ok(user) => user,
                Err(_) => {
                    return Err((
                        StatusCode::UNAUTHORIZED,
                        Json(ErrorResponse::new(ErrorCode::Unauthorized, "用户不存在")),
                    ));
                }
            }
        },
        Err(_) => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ErrorResponse::new(ErrorCode::Unauthorized, "用户不存在")),
            ));
        }
    };

    // 获取查询参数
    let since_timestamp = params
        .get("since")
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or(0);

    // 查询发送给当前用户的消息（离线消息）
    // 使用 im_single_message 表，直接使用 open_id 查询
    // 重要：过滤掉通话邀请消息（message_content_type = 4），因为通话邀请是实时消息，过期后没有意义
    let current_open_id = current_user.get_external_id();
    let messages = match sqlx::query_as::<_, ImSingleMessageRow>(
        "SELECT message_id, from_id, to_id, message_body, message_time, 
                message_content_type, read_status, extra, del_flag, sequence,
                message_random, create_time, update_time, version, reply_to,
                to_type, file_url, file_name, file_type
         FROM im_single_message
         WHERE to_id = ? AND message_time > ? AND del_flag = 1 AND message_content_type != 4
         ORDER BY message_time ASC
         LIMIT 100"
    )
    .bind(&current_open_id)
    .bind(since_timestamp)
    .fetch_all(&pool)
    .await
    {
        Ok(rows) => rows,
        Err(e) => {
            tracing::error!(error = %e, "查询离线消息失败");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::Internal, "查询消息失败")),
            ));
        }
    };

    // 转换为 ChatMessage 格式
    let mut result = Vec::new();
    for row in messages {
        result.push(json!({
            "message_id": row.message_id,
            "from_user_id": row.from_id,
            "to_user_id": row.to_id,
            "message": row.message_body,
            "timestamp_ms": row.message_time,
            "file_url": row.file_url,
            "file_name": row.file_name,
            "file_type": row.file_type,
        }));
    }

    Ok(Json(json!({ "messages": result })))
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)]
struct ImSingleMessageRow {
    message_id: String,
    from_id: String,
    to_id: String,
    message_body: String,
    message_time: i64,
    message_content_type: i32,
    read_status: i32,
    extra: Option<String>,
    del_flag: i16,
    sequence: i64,
    message_random: Option<String>,
    create_time: Option<i64>,
    update_time: Option<i64>,
    version: Option<i64>,
    reply_to: Option<String>,
    to_type: Option<String>,
    file_url: Option<String>,
    file_name: Option<String>,
    file_type: Option<String>,
}

