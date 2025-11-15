use axum::{extract::{Path, Extension, Query, State}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn, error};
use im_share::{mqtt_user_topic, ChatMessage, encode_message};
use crate::{
    error::{ErrorCode, ErrorResponse},
    service::{ImFriendshipService, UserService, SubscriptionService},
    model::ImFriendshipRequest,
    mqtt::MqttPublisher,
};

#[derive(Deserialize)]
pub struct AddFriendRequest {
    pub to_id: String,
    pub remark: Option<String>,
    pub add_source: Option<String>,
    pub message: Option<String>, // 好友验证信息
}

#[derive(Deserialize)]
pub struct HandleFriendshipRequest {
    pub approve_status: i32, // 1: 同意, 2: 拒绝
}

#[derive(Deserialize)]
pub struct UpdateRemarkRequest {
    pub remark: Option<String>,
}

/// 内部 API：根据 open_id 获取好友列表（不需要认证）
/// 用于 im-connect 服务获取用户的好友列表
pub async fn get_friends_by_open_id(
    Path(open_id): Path<String>,
    Extension(pool): Extension<MySqlPool>,
) -> impl IntoResponse {
    let service = ImFriendshipService::new(pool);
    
    match service.get_friends(&open_id).await {
        Ok(friends) => {
            let friends_with_info: Vec<serde_json::Value> = friends
                .into_iter()
                .map(|f| serde_json::json!({
                    "to_id": f.to_id,
                    "owner_id": f.owner_id,
                }))
                .collect();
            Ok(Json(serde_json::json!({"friends": friends_with_info})))
        },
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取好友列表失败")),
        )),
    }
}

/// 调试接口：查看数据库中实际存储的好友关系数据
/// 用于诊断好友列表为空的问题
pub async fn debug_friendship_data(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
) -> impl IntoResponse {
    use sqlx::Row;
    
    let user_service = UserService::new(pool.clone());
    
    // 获取当前用户信息
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let owner_id = user.get_external_id();
    
    // 查询所有可能的好友关系（不限制 owner_id 格式）
    let all_friendships = sqlx::query(
        "SELECT owner_id, to_id, del_flag, black, create_time 
         FROM im_friendship 
         WHERE (owner_id = ? OR owner_id = ? OR owner_id = ?) 
         AND (del_flag IS NULL OR del_flag = 1) 
         AND (black IS NULL OR black = 1)
         ORDER BY create_time DESC"
    )
    .bind(&owner_id)
    .bind(&user.name)
    .bind(user.open_id.as_ref().unwrap_or(&String::new()))
    .fetch_all(&pool)
    .await;
    
    let debug_info = serde_json::json!({
        "user": {
            "id": user.id,
            "name": user.name,
            "open_id": user.open_id,
            "phone": user.phone,
            "external_id": owner_id,
        },
        "friendships": match all_friendships {
            Ok(rows) => {
                let mut result = Vec::new();
                for row in rows {
                    result.push(serde_json::json!({
                        "owner_id": row.try_get::<String, _>("owner_id").unwrap_or_default(),
                        "to_id": row.try_get::<String, _>("to_id").unwrap_or_default(),
                        "del_flag": row.try_get::<Option<i32>, _>("del_flag").ok().flatten(),
                        "black": row.try_get::<Option<i32>, _>("black").ok().flatten(),
                        "create_time": row.try_get::<Option<i64>, _>("create_time").ok().flatten(),
                    }));
                }
                result
            },
            Err(e) => {
                warn!("查询好友关系失败: {:?}", e);
                Vec::new()
            }
        }
    });
    
    Ok(Json(debug_info))
}

pub async fn get_friends(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImFriendshipService::new(pool);
    
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
    
    let owner_id = user.get_external_id();
    info!("获取好友列表: user_id={}, owner_id={}, name={}, open_id={:?}", 
          user_id, owner_id, user.name, user.open_id);
    
    match service.get_friends(&owner_id).await {
        Ok(friends) => {
            // 为每个好友查询用户信息
            let mut friends_with_info = Vec::new();
            for friend in friends {
                // 根据 to_id（可能是用户名、手机号或 open_id）查询用户信息
                let friend_user = match user_service.get_by_name(&friend.to_id).await {
                    Ok(user) => Some(user),
                    Err(_) => match user_service.get_by_phone(&friend.to_id).await {
                        Ok(user) => Some(user),
                        Err(_) => user_service.get_by_open_id(&friend.to_id).await.ok(),
                    },
                };
                
                // 构建包含用户信息的好友对象
                let friend_info = serde_json::json!({
                    "to_id": friend.to_id,
                    "owner_id": friend.owner_id,
                    "remark": friend.remark,
                    "del_flag": friend.del_flag,
                    "black": friend.black,
                    "create_time": friend.create_time,
                    "update_time": friend.update_time,
                    "sequence": friend.sequence,
                    "black_sequence": friend.black_sequence,
                    "add_source": friend.add_source,
                    "extra": friend.extra,
                    "version": friend.version,
                    "user": friend_user.map(|u| serde_json::json!({
                        "id": u.id,
                        "open_id": u.open_id,
                        "name": u.name,
                        "email": u.email,
                        "file_name": u.file_name,
                        "abstract": u.abstract_field,
                        "phone": u.phone,
                        "gender": u.gender,
                    })),
                });
                friends_with_info.push(friend_info);
            }
            Ok(Json(serde_json::json!({"friends": friends_with_info})))
        },
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取好友列表失败")),
        )),
    }
}

pub async fn add_friend(
    State((publisher, subscription_service)): State<(MqttPublisher, Arc<SubscriptionService>)>,
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Json(req): Json<AddFriendRequest>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let friendship_service = ImFriendshipService::new(pool.clone());
    
    // 获取当前用户信息
    let from_user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            warn!("获取当前用户失败: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    // 统一使用 get_external_id() 获取外部标识，与 get_friends 保持一致
    // get_external_id() 的逻辑：优先用户名，其次手机号，最后 open_id
    let from_id = from_user.get_external_id();
    
    // 验证 to_id 必须是用户名、手机号或 open_id，并查找对应的用户
    let to_user = match user_service.get_by_name(&req.to_id).await {
        Ok(user) => Ok(user),
        Err(_) => match user_service.get_by_phone(&req.to_id).await {
            Ok(user) => Ok(user),
            Err(_) => user_service.get_by_open_id(&req.to_id).await,
        },
    };
    
    let to_user = match to_user {
        Ok(user) => user,
        Err(_) => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::NotFound, "未找到该用户，请确认用户名、手机号或Open ID是否正确")),
            ));
        }
    };
    
    // 使用 get_external_id() 获取接收者的外部标识，与 get_friends 保持一致
    let to_id = to_user.get_external_id();
    
    // 检查是否已经是好友
    info!("检查好友关系: from_id={}, to_id={}, from_user.name={}, from_user.open_id={:?}, to_user.name={}, to_user.open_id={:?}", 
          from_id, to_id, from_user.name, from_user.open_id, to_user.name, to_user.open_id);
    if let Ok(is_friend) = friendship_service.is_friend(&from_id, &to_id).await {
        if is_friend {
            warn!("已经是好友: from_id={}, to_id={}", from_id, to_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::InvalidInput, "已经是好友了")),
            ));
        }
    }
    
    // 检查是否已经有待处理的好友请求（双向检查）
    // 注意：只检查待处理的请求（approve_status = 0），已拒绝的请求（approve_status = 2）可以重新发送
    
    // 1. 检查是否已经向对方发送过待处理的请求（from_id -> to_id）
    let existing_requests_to = friendship_service.get_friendship_requests(&to_id, Some(0)).await;
    if let Ok(requests) = existing_requests_to {
        // 只检查待处理的请求（approve_status = 0）
        if requests.iter().any(|r| r.from_id == from_id && r.approve_status == Some(0)) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::InvalidInput, "已经发送过好友请求，等待对方处理")),
            ));
        }
    }
    
    // 2. 检查对方是否已经向自己发送过待处理的请求（to_id -> from_id），如果是，应该提示用户直接同意
    let existing_requests_from = friendship_service.get_friendship_requests(&from_id, Some(0)).await;
    if let Ok(requests) = existing_requests_from {
        // 只检查待处理的请求（approve_status = 0）
        if requests.iter().any(|r| r.from_id == to_id && r.approve_status == Some(0)) {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::InvalidInput, "对方已经向您发送过好友请求，请先处理对方的请求")),
            ));
        }
    }
    
    // 3. 如果之前有被拒绝的请求，允许重新发送（删除旧请求或创建新请求）
    // 检查是否有被拒绝的请求（approve_status = 2）
    let _rejected_requests_to = friendship_service.get_friendship_requests(&to_id, Some(2)).await;
    // 如果有被拒绝的请求，可以重新发送（创建新请求会覆盖旧请求）
    // 这里不做任何处理，允许继续创建新请求
    
    // 生成好友请求ID
    use uuid::Uuid;
    let request_id = Uuid::new_v4().to_string();
    
    // 创建好友请求（先克隆需要后续使用的字段）
    let from_id_clone = from_id.clone();
    let to_id_clone = to_id.clone();
    let remark_clone = req.remark.clone();
    let add_source_clone = req.add_source.clone();
    let message_clone = req.message.clone();
    
    let friendship_request = ImFriendshipRequest {
        id: request_id.clone(),
        from_id,
        to_id,
        remark: req.remark,
        read_status: Some(0),
        add_source: req.add_source,
        message: req.message,
        approve_status: Some(0), // 0: 待处理
        create_time: Some(im_share::now_timestamp()),
        update_time: Some(im_share::now_timestamp()),
        sequence: Some(im_share::now_timestamp()),
        del_flag: Some(1),
        version: Some(1),
    };
    
    match friendship_service.create_friendship_request(friendship_request).await {
        Ok(_) => {
            info!("创建好友请求成功: request_id={}, from_id={}, to_id={}", request_id, from_id_clone, to_id_clone);
            
            // 通过 MQTT 推送好友请求通知给接收者
            // to_id_clone 已经是用户名或手机号，直接查找
            let to_user = match user_service.get_by_name(&to_id_clone).await {
                Ok(user) => Ok(user),
                Err(_) => user_service.get_by_phone(&to_id_clone).await,
            };
            
            if let Ok(to_user) = to_user {
                // 获取接收者的订阅ID
                let subscription_ids = {
                    let mut ids = subscription_service.get_subscription_ids(to_user.id);
                    // 如果内存中没有，从数据库查询（只查询最近24小时内创建的订阅，过滤掉已不在线的用户）
                    if ids.is_empty() {
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
                
                // 构建好友请求通知消息
                let notification_message = ChatMessage {
                    message_id: request_id.clone(),
                    from_user_id: from_id_clone.clone(),
                    to_user_id: to_id_clone.clone(),
                    message: format!(
                        r#"{{"type":"friendship_request","request_id":"{}","from_id":"{}","to_id":"{}","remark":{},"message":{},"add_source":{}}}"#,
                        request_id,
                        from_id_clone,
                        to_id_clone,
                        remark_clone.as_ref().map(|r| format!("\"{}\"", r)).unwrap_or_else(|| "null".to_string()),
                        message_clone.as_ref().map(|m| format!("\"{}\"", m)).unwrap_or_else(|| "null".to_string()),
                        add_source_clone.as_ref().map(|s| format!("\"{}\"", s)).unwrap_or_else(|| "null".to_string())
                    ),
                    timestamp_ms: im_share::now_timestamp(),
                    file_url: None,
                    file_name: None,
                    file_type: None,
                    chat_type: Some(1), // 1 = 单聊（好友请求也是单聊的一种）
                };
                
                // 无论用户是否在线，都通过 MQTT 发布通知
                // MQTT broker 会自动处理离线消息（使用 QoS 1 和 clean_session=false）
                let to_mqtt_id = to_user.get_mqtt_id();
                let topic = mqtt_user_topic(&to_mqtt_id.to_string());
                let is_online = !subscription_ids.is_empty();
                info!(to_id = %to_id_clone, is_online = is_online, %topic, "通过MQTT发布好友请求通知（broker会自动处理离线消息）");
                
                match encode_message(&notification_message) {
                    Ok(payload) => {
                        if let Err(e) = publisher.publish(&topic, payload).await {
                            error!(to_id = %to_id_clone, %topic, error = %e, "好友请求MQTT发布失败");
                        } else {
                            info!(to_id = %to_id_clone, %topic, is_online = is_online, "好友请求已通过MQTT发布（如果用户离线，broker会存储消息）");
                        }
                    }
                    Err(e) => {
                        error!(error = %e, "好友请求消息编码失败");
                    }
                }
            } else {
                warn!(to_id = %to_id_clone, "无法获取接收者信息，无法通过MQTT发送好友请求通知");
                // 无法获取用户信息时，无法通过 MQTT 发布，但好友请求已保存到数据库
                // 用户可以通过查询好友请求列表来获取
            }
            
            Ok(Json(serde_json::json!({"status": "ok", "request_id": request_id, "message": "好友请求已发送，等待对方同意"})))
        },
        Err(e) => {
            warn!("创建好友请求失败: {:?}", e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, "发送好友请求失败")),
            ))
        }
    }
}

pub async fn remove_friend(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(to_id): Path<String>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImFriendshipService::new(pool.clone());
    
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
    
    let owner_id = user.get_external_id();
    
    // 将 to_id 转换为 open_id（支持用户名、手机号、open_id、snowflake_id）
    let to_user = match user_service.get_by_name(&to_id).await {
        Ok(user) => Ok(user),
        Err(_) => match user_service.get_by_phone(&to_id).await {
            Ok(user) => Ok(user),
            Err(_) => user_service.get_by_open_id(&to_id).await,
        },
    };
    
    let to_user = match to_user {
        Ok(user) => user,
        Err(_) => {
            warn!("无法找到要删除的好友: to_id={}", to_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::NotFound, "未找到该用户，请确认用户ID或用户名是否正确")),
            ));
        }
    };
    
    let to_id_open_id = to_user.get_external_id();
    info!("删除好友: owner_id={}, to_id={}, to_id_open_id={}", owner_id, to_id, to_id_open_id);
    
    // 使用转换后的 open_id 删除好友
    match service.remove_friend(&owner_id, &to_id_open_id).await {
        Ok(_) => {
            info!("成功删除好友: owner_id={}, to_id={}", owner_id, to_id_open_id);
            Ok(Json(serde_json::json!({"status": "ok", "message": "好友删除成功"})))
        },
        Err(e) => {
            warn!("删除好友失败: owner_id={}, to_id={}, error={:?}", owner_id, to_id_open_id, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, "删除好友失败")),
            ))
        },
    }
}

pub async fn update_remark(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(to_id): Path<String>,
    Json(req): Json<UpdateRemarkRequest>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImFriendshipService::new(pool);
    
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
    
    let owner_id = user.get_external_id();
    
    match service.update_remark(&owner_id, &to_id, req.remark).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "更新备注失败")),
        )),
    }
}

pub async fn black_friend(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(to_id): Path<String>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImFriendshipService::new(pool);
    
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
    
    let owner_id = user.get_external_id();
    
    match service.black_friend(&owner_id, &to_id).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "拉黑好友失败")),
        )),
    }
}

pub async fn create_friendship_request(
    Extension(pool): Extension<MySqlPool>,
    Json(req): Json<ImFriendshipRequest>,
) -> impl IntoResponse {
    let service = ImFriendshipService::new(pool);
    
    match service.create_friendship_request(req).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "创建好友请求失败")),
        )),
    }
}

pub async fn get_friendship_requests(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());
    let service = ImFriendshipService::new(pool.clone());
    
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
    let approve_status = params.get("approve_status").and_then(|s| s.parse::<i32>().ok());
    
    match service.get_friendship_requests(&to_id, approve_status).await {
        Ok(requests) => {
            // 为每个请求查询发送者的用户信息
            let mut requests_with_info = Vec::new();
            for request in requests {
                // 根据 from_id 查询用户信息
                // 注意：from_id 在数据库中通常存储的是 open_id
                // 所以优先使用 get_by_open_id 查询，如果失败再尝试其他方式
                let from_user = match user_service.get_by_open_id(&request.from_id).await {
                    Ok(user) => {
                        info!(
                            from_id = %request.from_id,
                            user_name = %user.name,
                            "通过 open_id 查询到用户信息"
                        );
                        Some(user)
                    },
                    Err(_) => {
                        // 如果 open_id 查询失败，尝试作为用户名查询
                        match user_service.get_by_name(&request.from_id).await {
                            Ok(user) => {
                                info!(
                                    from_id = %request.from_id,
                                    user_name = %user.name,
                                    "通过用户名查询到用户信息"
                                );
                                Some(user)
                            },
                            Err(_) => {
                                // 最后尝试作为手机号查询
                                match user_service.get_by_phone(&request.from_id).await {
                                    Ok(user) => {
                                        info!(
                                            from_id = %request.from_id,
                                            user_name = %user.name,
                                            "通过手机号查询到用户信息"
                                        );
                                        Some(user)
                                    },
                                    Err(e) => {
                                        warn!(
                                            from_id = %request.from_id,
                                            error = ?e,
                                            "无法查询到用户信息（尝试了 open_id、用户名、手机号）"
                                        );
                                        None
                                    },
                                }
                            },
                        }
                    },
                };
                
                // 构建包含用户信息的请求对象
                let request_info = serde_json::json!({
                    "id": request.id,
                    "from_id": request.from_id,
                    "to_id": request.to_id,
                    "remark": request.remark,
                    "read_status": request.read_status,
                    "add_source": request.add_source,
                    "message": request.message,
                    "approve_status": request.approve_status,
                    "create_time": request.create_time,
                    "update_time": request.update_time,
                    "sequence": request.sequence,
                    "del_flag": request.del_flag,
                    "version": request.version,
                    "user": from_user.map(|u| serde_json::json!({
                        "id": u.id,
                        "open_id": u.open_id,
                        "name": u.name,
                        "email": u.email,
                        "file_name": u.file_name,
                        "abstract": u.abstract_field,
                        "phone": u.phone,
                        "gender": u.gender,
                    })),
                });
                requests_with_info.push(request_info);
            }
            Ok(Json(serde_json::json!({"requests": requests_with_info})))
        },
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取好友请求失败")),
        )),
    }
}

pub async fn handle_friendship_request(
    Extension(pool): Extension<MySqlPool>,
    Path(request_id): Path<String>,
    Json(req): Json<HandleFriendshipRequest>,
) -> impl IntoResponse {
    let service = ImFriendshipService::new(pool);
    
    match service.handle_friendship_request(&request_id, req.approve_status).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "处理好友请求失败")),
        )),
    }
}

