use axum::{extract::{Path, Extension, State}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use serde::Deserialize;
use std::sync::Arc;
use crate::{
    error::{ErrorCode, ErrorResponse},
    service::{ImGroupService, ImChatService, UpdateGroupRequest, SubscriptionService},
    model::ImGroup,
    mqtt::MqttPublisher,
    redis::RedisClient,
};

#[derive(Deserialize)]
pub struct CreateGroupRequest {
    pub group_id: String,
    pub group_name: String,
    pub group_type: i32,
    pub apply_join_type: i32,
    pub avatar: Option<String>,
    pub max_member_count: Option<i32>,
    pub introduction: Option<String>,
    pub notification: Option<String>,
    pub verifier: Option<i16>,
}

#[derive(Deserialize)]
pub struct AddGroupMemberRequest {
    #[allow(dead_code)]
    pub member_id: String,
    pub role: Option<i32>,
    pub alias: Option<String>,
}

pub async fn create_group(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    use std::time::{SystemTime, UNIX_EPOCH};
    use tracing::{warn, info, error};
    
    info!("创建群组请求: group_id={}, group_name={}, group_type={}, apply_join_type={}, introduction={:?}", 
          req.group_id, req.group_name, req.group_type, req.apply_join_type, req.introduction);
    
    // 验证请求参数
    if req.group_id.is_empty() {
        warn!("群组ID为空");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "群组ID不能为空")),
        ));
    }
    if req.group_name.trim().is_empty() {
        warn!("群组名称为空");
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "群组名称不能为空")),
        ));
    }
    
    let user_service = crate::service::UserService::new(pool.clone());
    let service = ImGroupService::new(pool.clone());
    let chat_service = ImChatService::new(pool.clone());
    
    // 获取用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let owner_id = user.get_external_id();
    info!("群主ID: {}", owner_id);
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    
    let group = ImGroup {
        group_id: req.group_id,
        owner_id: owner_id.clone(),
        group_type: req.group_type,
        group_name: req.group_name.trim().to_string(),
        mute: Some(1),
        apply_join_type: req.apply_join_type,
        avatar: req.avatar.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        max_member_count: req.max_member_count,
        introduction: req.introduction.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        notification: req.notification.map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
        status: Some(1),
        sequence: Some(0),
        create_time: Some(now),
        update_time: Some(now),
        extra: None,
        version: Some(1),
        del_flag: 1,
        verifier: req.verifier,
        member_count: Some(1),
    };
    
    match service.create_group(group.clone()).await {
        Ok(_) => {
            // 为群主创建聊天记录（如果还没有的话）
            // 这样即使没有发送过消息，群组也会出现在聊天列表中
            let chat_id = format!("group_{}", group.group_id);
            
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                2, // chat_type: 2 = 群聊
                owner_id.clone(),
                group.group_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, owner_id = %owner_id, error = ?e, "为群主创建聊天记录失败（不影响创建群组）");
            } else {
                info!(chat_id = %chat_id, owner_id = %owner_id, "已为群主创建聊天记录");
            }
            
            Ok(Json(serde_json::json!({"status": "ok"})))
        },
        Err(e) => {
            let error_msg = match e {
                ErrorCode::InvalidInput => "创建群组失败：输入参数无效",
                ErrorCode::Database => "创建群组失败：数据库错误",
                _ => "创建群组失败",
            };
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

pub async fn get_user_groups(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
) -> impl IntoResponse {
    use tracing::{info, warn};
    use crate::service::UserService;
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            warn!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let owner_id = user.get_external_id();
    info!("获取用户群组列表: user_id={}, owner_id={}", user_id, owner_id);
    
    match group_service.get_user_groups(&owner_id).await {
        Ok(groups) => {
            info!("成功获取用户群组列表: owner_id={}, count={}", owner_id, groups.len());
            Ok(Json(serde_json::json!({"groups": groups})))
        },
        Err(e) => {
            warn!("获取用户群组列表失败: owner_id={}, error={:?}", owner_id, e);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取群组列表失败")),
            ))
        }
    }
}

pub async fn get_group(
    Extension(pool): Extension<MySqlPool>,
    Path(group_id): Path<String>,
) -> impl IntoResponse {
    let service = ImGroupService::new(pool);
    
    match service.get_group(&group_id).await {
        Ok(group) => Ok(Json(group)),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(e, "群组不存在")),
        )),
    }
}

pub async fn get_group_members(
    Extension(pool): Extension<MySqlPool>,
    Path(group_id): Path<String>,
) -> impl IntoResponse {
    let service = ImGroupService::new(pool);
    
    match service.get_group_members(&group_id).await {
        Ok(members) => Ok(Json(serde_json::json!({"members": members}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取群成员失败")),
        )),
    }
}

pub async fn add_group_member(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path((group_id, member_id)): Path<(String, String)>,
    Json(req): Json<AddGroupMemberRequest>,
) -> impl IntoResponse {
    use tracing::{info, warn};
    use crate::service::{ImGroupService, ImFriendshipService, UserService};
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool.clone());
    let friendship_service = ImFriendshipService::new(pool.clone());
    
    // 获取当前用户信息，优先使用 open_id，否则使用 snowflake_id
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
    
    let from_id = from_user.get_external_id();
    
    // 将 member_id 转换为 open_id（支持用户名、手机号、open_id、snowflake_id）
    let to_user = match user_service.get_by_name(&member_id).await {
        Ok(user) => Ok(user),
        Err(_) => match user_service.get_by_phone(&member_id).await {
            Ok(user) => Ok(user),
            Err(_) => user_service.get_by_open_id(&member_id).await,
        },
    };
    
    let to_user = match to_user {
        Ok(user) => user,
        Err(_) => {
            warn!("无法找到成员用户: member_id={}", member_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::NotFound, "未找到该用户，请确认用户ID或用户名是否正确")),
            ));
        }
    };
    
    let to_id = to_user.get_external_id();
    info!("添加群成员: from_id={}, member_id={}, to_id={}", from_id, member_id, to_id);
    
    // 检查是否是好友，只有好友才能直接拉入群组
    match friendship_service.is_friend(&from_id, &to_id).await {
        Ok(true) => {
            info!("用户 {} 和 {} 是好友，可以直接添加到群组", from_id, to_id);
        },
        Ok(false) => {
            warn!("用户 {} 和 {} 不是好友，无法添加到群组", from_id, to_id);
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::InvalidInput, "只能添加好友到群组")),
            ));
        },
        Err(e) => {
            warn!("检查好友关系失败: {:?}", e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "检查好友关系失败")),
            ));
        }
    }
    
    // 在添加成员前，检查当前成员数
    let current_members = match group_service.get_group_members(&group_id).await {
        Ok(members) => members,
        Err(_) => {
            // 如果获取成员失败，可能是群组不存在，继续尝试添加（可能是新群组）
            vec![]
        }
    };
    
    let current_member_count = current_members.len();
    // 添加成员后，成员数会变成 current_member_count + 1
    let will_have_more_than_2_members = current_member_count >= 2;
    
    info!("添加成员前: group_id={}, current_member_count={}, will_have_more_than_2_members={}", 
          group_id, current_member_count, will_have_more_than_2_members);
    
    // 如果成员数大于2（即3人及以上），需要确保group_id存在且唯一
    let final_group_id = if will_have_more_than_2_members {
        // 检查群组是否存在
        match group_service.get_group(&group_id).await {
            Ok(_) => {
                // 群组已存在，使用现有group_id
                info!("群组已存在，使用现有group_id: {}", group_id);
                group_id
            },
            Err(_) => {
                // 群组不存在，创建一个新的唯一group_id
                use uuid::Uuid;
                let new_group_id = Uuid::new_v4().to_string();
                info!("群组不存在，创建新的唯一group_id: {} -> {}", group_id, new_group_id);
                
                // 创建群组记录
                use crate::model::ImGroup;
                use std::time::{SystemTime, UNIX_EPOCH};
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as i64;
                
                // 获取群主（从现有成员中找角色最高的，或者使用第一个成员）
                let owner_id = current_members.iter()
                    .find(|m| m.role == 2)
                    .map(|m| m.member_id.clone())
                    .or_else(|| current_members.first().map(|m| m.member_id.clone()))
                    .unwrap_or_else(|| from_id.clone());
                
                let new_group = ImGroup {
                    group_id: new_group_id.clone(),
                    owner_id: owner_id.clone(),
                    group_type: 1, // 私有群
                    group_name: format!("群聊"), // 默认名称，前端可以修改
                    mute: Some(0),
                    apply_join_type: 1,
                    avatar: None,
                    max_member_count: None,
                    introduction: None,
                    notification: None,
                    status: Some(1),
                    sequence: Some(0),
                    create_time: Some(now),
                    update_time: Some(now),
                    extra: None,
                    version: Some(1),
                    del_flag: 1,
                    verifier: None,
                    member_count: None,
                };
                
                if let Err(e) = group_service.create_group(new_group).await {
                    warn!("创建群组失败: {:?}", e);
                    // 如果创建失败，继续使用原group_id（可能是临时ID）
                    group_id
                } else {
                    info!("成功创建新群组: {}", new_group_id);
                    
                    // 如果原group_id已有成员，需要更新这些成员的group_id到新的group_id
                    if !current_members.is_empty() {
                        // 更新所有现有成员的group_id（使用group_service的pool）
                        // 注意：需要更新group_member_id和group_id
                        for member in &current_members {
                            // 先删除旧的成员记录，然后插入新的（或者更新）
                            // 使用group_service的方法来更新
                            if let Err(e) = group_service.add_group_member(&new_group_id, &member.member_id, member.role, member.alias.clone()).await {
                                warn!("迁移成员到新group_id失败: member_id={}, error={:?}", member.member_id, e);
                            } else {
                                info!("成功迁移成员到新group_id: {} -> {}, member_id={}", group_id, new_group_id, member.member_id);
                            }
                        }
                    }
                    
                    new_group_id
                }
            }
        }
    } else {
        group_id
    };
    
    let role = req.role.unwrap_or(0);
    // 使用最终的group_id添加群成员
    match group_service.add_group_member(&final_group_id, &to_id, role, req.alias).await {
        Ok(_) => {
            info!("成功将用户 {} 添加到群组 {}", member_id, final_group_id);
            
            // 为新加入的成员创建聊天记录（如果还没有的话）
            // 这样即使没有发送过消息，群组也会出现在聊天列表中
            let chat_service = ImChatService::new(pool.clone());
            let chat_id = format!("group_{}", final_group_id);
            
            if let Err(e) = chat_service.get_or_create_chat(
                chat_id.clone(),
                2, // chat_type: 2 = 群聊
                to_id.clone(),
                final_group_id.clone(),
            ).await {
                warn!(chat_id = %chat_id, member_id = %to_id, error = ?e, "为新加入的群成员创建聊天记录失败（不影响加入群组）");
            } else {
                info!(chat_id = %chat_id, member_id = %to_id, "已为新加入的群成员创建聊天记录");
            }
            
            Ok(Json(serde_json::json!({
                "status": "ok", 
                "message": "已成功添加好友到群组",
                "group_id": final_group_id
            })))
        },
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "添加群成员失败")),
        )),
    }
}

pub async fn remove_group_member(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path((group_id, member_id)): Path<(String, String)>,
) -> impl IntoResponse {
    use tracing::{info, warn, error};
    use crate::service::{ImGroupService, UserService};
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let operator_id = user.get_external_id();
    info!("移除群成员请求: group_id={}, member_id={}, operator_id={}", group_id, member_id, operator_id);
    
    match group_service.remove_group_member(&group_id, &member_id, &operator_id).await {
        Ok(_) => {
            info!("成功移除群成员: group_id={}, member_id={}, operator_id={}", group_id, member_id, operator_id);
            Ok(Json(serde_json::json!({"status": "ok", "message": "已移除成员"})))
        },
        Err(e) => {
            let error_msg = match e {
                ErrorCode::InvalidInput => "只有群主或管理员可以移除成员，且不能移除群主",
                ErrorCode::NotFound => "成员不存在",
                _ => "移除群成员失败",
            };
            warn!("移除群成员失败: group_id={}, member_id={}, operator_id={}, error={:?}", 
                  group_id, member_id, operator_id, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: i32, // 0=普通成员，1=管理员，2=群主
}

pub async fn update_member_role(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path((group_id, member_id)): Path<(String, String)>,
    Json(req): Json<UpdateMemberRoleRequest>,
) -> impl IntoResponse {
    use tracing::{info, warn, error};
    use crate::service::{ImGroupService, UserService};
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let operator_id = user.get_external_id();
    info!("更新群成员角色请求: group_id={}, member_id={}, role={}, operator_id={}", 
          group_id, member_id, req.role, operator_id);
    
    match group_service.update_member_role(&group_id, &member_id, req.role, &operator_id).await {
        Ok(_) => {
            let role_name = match req.role {
                0 => "普通成员",
                1 => "管理员",
                2 => "群主",
                _ => "未知",
            };
            info!("成功更新群成员角色: group_id={}, member_id={}, role={}", group_id, member_id, role_name);
            Ok(Json(serde_json::json!({"status": "ok", "message": format!("已设置为{}", role_name)})))
        },
        Err(e) => {
            let error_msg = match e {
                ErrorCode::InvalidInput => "只有群主才能修改成员角色，或角色值无效",
                ErrorCode::NotFound => "群组或成员不存在",
                _ => "更新成员角色失败",
            };
            warn!("更新群成员角色失败: group_id={}, member_id={}, role={}, operator_id={}, error={:?}", 
                  group_id, member_id, req.role, operator_id, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateMemberAliasRequest {
    pub alias: Option<String>,
}

pub async fn update_member_alias(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path((group_id, member_id)): Path<(String, String)>,
    Json(req): Json<UpdateMemberAliasRequest>,
) -> impl IntoResponse {
    use tracing::{info, warn, error};
    use crate::service::{ImGroupService, UserService};
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let operator_id = user.get_external_id();
    
    // 验证：只能修改自己的别名
    if member_id != operator_id {
        warn!("用户 {} 尝试修改其他成员的别名: group_id={}, member_id={}", operator_id, group_id, member_id);
        return Err((
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "只能修改自己的群昵称")),
        ));
    }
    
    info!("更新群成员别名请求: group_id={}, member_id={}, alias={:?}", group_id, member_id, req.alias);
    
    match group_service.update_member_alias(&group_id, &member_id, req.alias.clone()).await {
        Ok(_) => {
            info!("成功更新群成员别名: group_id={}, member_id={}, alias={:?}", group_id, member_id, req.alias);
            Ok(Json(serde_json::json!({"status": "ok", "message": "群昵称已更新"})))
        },
        Err(e) => {
            let error_msg = match e {
                ErrorCode::NotFound => "群组或成员不存在",
                ErrorCode::InvalidInput => "无效的输入",
                _ => "更新群昵称失败",
            };
            warn!("更新群成员别名失败: group_id={}, member_id={}, alias={:?}, error={:?}", 
                  group_id, member_id, req.alias, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

pub async fn delete_group(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(group_id): Path<String>,
) -> impl IntoResponse {
    use tracing::{info, warn, error};
    use crate::service::{ImGroupService, UserService};
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let owner_id = user.get_external_id();
    info!("删除群组请求: group_id={}, owner_id={}", group_id, owner_id);
    
    match group_service.delete_group(&group_id, &owner_id).await {
        Ok(_) => {
            info!("成功删除群组: group_id={}, owner_id={}", group_id, owner_id);
            Ok(Json(serde_json::json!({"status": "ok", "message": "群组已删除"})))
        },
        Err(e) => {
            let error_msg = match e {
                ErrorCode::InvalidInput => "只有群主才能删除群组",
                ErrorCode::NotFound => "群组不存在",
                _ => "删除群组失败",
            };
            warn!("删除群组失败: group_id={}, owner_id={}, error={:?}", group_id, owner_id, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

pub async fn dissolve_group(
    State((publisher, subscription_service)): State<(MqttPublisher, Arc<SubscriptionService>)>,
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Extension(user_id): Extension<u64>,
    Path(group_id): Path<String>,
) -> impl IntoResponse {
    use tracing::{info, warn, error};
    use crate::service::{ImGroupService, UserService, ImMessageService};
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;
    use im_share::{ChatMessage, mqtt_user_topic, encode_message};
    use std::collections::HashSet;
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool.clone());
    let message_service = ImMessageService::with_redis(pool.clone(), redis_client.clone());
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let owner_id = user.get_external_id();
    
    info!(
        "解散群组请求: group_id={}, owner_id='{}', user_id={}, user_name='{}', user_open_id='{:?}', user_snowflake_id='{:?}'",
        group_id, 
        owner_id,
        user_id,
        user.name,
        user.open_id,
        user.id
    );
    
    // 在解散前获取群组名称
    let group_name = match group_service.get_group(&group_id).await {
        Ok(g) => g.group_name,
        Err(_) => group_id.clone(),
    };
    
    match group_service.dissolve_group(&group_id, &owner_id).await {
        Ok(members) => {
            info!("成功解散群组: group_id={}, owner_id={}, member_count={}", group_id, owner_id, members.len());
            
            // 发送系统消息通知所有成员群组已解散
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let message_id = Uuid::new_v4().to_string();
            let system_message = format!(r#"{{"type":"group_dissolved","group_id":"{}","group_name":"{}","owner_id":"{}"}}"#, 
                group_id, group_name, owner_id);
            
            // 保存系统消息到数据库
            let normalized_group_id = if group_id.starts_with("group_") {
                group_id.clone()
            } else {
                format!("group_{}", group_id)
            };
            
            use crate::model::ImGroupMessage;
            let group_message = ImGroupMessage {
                message_id: message_id.clone(),
                group_id: normalized_group_id.clone(),
                from_id: "system".to_string(),
                message_body: system_message.clone(),
                message_time: now,
                message_content_type: 100, // 系统消息类型
                extra: None,
                del_flag: 1,
                sequence: Some(now),
                message_random: Some(Uuid::new_v4().to_string()),
                create_time: now,
                update_time: Some(now),
                version: Some(1),
                reply_to: None,
            };
            
            if let Err(e) = message_service.save_group_message(group_message).await {
                warn!("保存群组解散系统消息失败: group_id={}, error={:?}", group_id, e);
            }
            
            // 去重：使用 HashSet 确保每个 member_id 只处理一次
            let mut processed_member_ids = HashSet::new();
            
            // 为每个群成员推送系统消息
            for member in &members {
                let member_id_str = &member.member_id;
                
                // 获取成员用户信息
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
                
                let member_open_id = member_user.get_external_id();
                
                // 如果已经处理过这个成员，跳过（去重）
                if !processed_member_ids.insert(member_open_id.clone()) {
                    continue;
                }
                
                // 构建系统消息
                let chat_message = ChatMessage {
                    message_id: message_id.clone(),
                    from_user_id: "system".to_string(),
                    to_user_id: normalized_group_id.clone(),
                    message: system_message.clone(),
                    timestamp_ms: now,
                    file_url: None,
                    file_name: None,
                    file_type: None,
                    chat_type: Some(2), // 群聊
                };
                
                // 获取成员的MQTT ID
                let member_mqtt_id = member_user.get_mqtt_id();
                
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
                
                // 通过 MQTT 推送系统消息
                let topic = mqtt_user_topic(&member_mqtt_id.to_string());
                let is_online = !subscription_ids.is_empty();
                info!(member_id = %member_open_id, is_online = is_online, %topic, "通过MQTT推送群组解散系统消息");
                
                match encode_message(&chat_message) {
                    Ok(payload) => {
                        if let Err(e) = publisher.publish(&topic, payload).await {
                            warn!(member_id = %member_open_id, error = ?e, "推送群组解散系统消息失败");
                        }
                    },
                    Err(e) => {
                        warn!(member_id = %member_open_id, error = ?e, "编码群组解散系统消息失败");
                    }
                }
            }
            
            Ok(Json(serde_json::json!({"status": "ok", "message": "群组已解散"})))
        },
        Err(e) => {
            let error_msg = match e {
                ErrorCode::InvalidInput => "只有群主才能解散群组",
                ErrorCode::NotFound => "群组不存在",
                _ => "解散群组失败",
            };
            warn!("解散群组失败: group_id={}, owner_id={}, error={:?}", group_id, owner_id, e);
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

pub async fn update_group(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(group_id): Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> impl IntoResponse {
    use tracing::{info, warn, error};
    use crate::service::{ImGroupService, UserService};
    
    let user_service = UserService::new(pool.clone());
    let group_service = ImGroupService::new(pool);
    
    // 获取当前用户信息，使用外部 ID（open_id 或 snowflake_id）
    let user = match user_service.get_by_id(user_id).await {
        Ok(user) => user,
        Err(e) => {
            error!("获取当前用户失败: user_id={}, error={:?}", user_id, e);
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(e, "获取用户信息失败")),
            ));
        }
    };
    
    let owner_id = user.get_external_id();
    info!("更新群组信息请求: group_id={}, owner_id={}", group_id, owner_id);
    
    match group_service.update_group(&group_id, &owner_id, &req).await {
        Ok(_) => {
            info!("成功更新群组信息: group_id={}, owner_id={}", group_id, owner_id);
            Ok(Json(serde_json::json!({"status": "ok", "message": "群组信息已更新"})))
        },
        Err(e) => {
            let (status_code, error_msg) = match e {
                ErrorCode::InvalidInput => (
                    StatusCode::BAD_REQUEST,
                    "只有群主才能更新群组信息，或输入参数无效"
                ),
                ErrorCode::NotFound => (
                    StatusCode::NOT_FOUND,
                    "群组不存在"
                ),
                _ => (
                    StatusCode::BAD_REQUEST,
                    "更新群组信息失败"
                ),
            };
            warn!("更新群组信息失败: group_id={}, owner_id={}, error={:?}", group_id, owner_id, e);
            Err((
                status_code,
                Json(ErrorResponse::new(e, error_msg)),
            ))
        }
    }
}

