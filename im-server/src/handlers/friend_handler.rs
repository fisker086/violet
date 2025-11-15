use axum::{extract::{Path, Extension}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use tracing::{info, warn};
use crate::{
    error::{ErrorCode, ErrorResponse},
    service::{FriendService, UserService},
};

/// 添加好友
pub async fn add_friend(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(friend_id_str): Path<String>,
) -> impl IntoResponse {
    info!("添加好友请求: user_id={}, friend_id_str={}", user_id, friend_id_str);
    
    let user_service = UserService::new(pool.clone());
    let friend_id: u64 = match friend_id_str.parse::<u64>() {
        Ok(id) => {
            // 如果是数字，先尝试作为 open_id 查询，如果失败再尝试作为数据库ID查询
            let open_id = id.to_string();
            match user_service.get_by_open_id(&open_id).await {
                Ok(user) => {
                    info!("通过 open_id 找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
                    user.id
                },
                Err(ErrorCode::NotFound) => {
                    // 如果通过 open_id 获取失败，尝试作为数据库ID查询
                    match user_service.get_by_id(id).await {
                        Ok(user) => {
                            info!("通过数据库ID找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
                            user.id
                        },
                        Err(_) => {
                            warn!("无法找到用户: {}", id);
                            return Err((
                                StatusCode::NOT_FOUND,
                                Json(ErrorResponse::new(ErrorCode::NotFound, "用户不存在")),
                            ));
                        }
                    }
                },
                Err(code) => {
                    warn!("查询用户时发生错误: id={}, error={:?}", id, code);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(code, "查询用户失败")),
                    ));
                }
            }
        },
        Err(_) => {
            // 如果不是数字，尝试通过 open_id（字符串）或用户名查找
            match user_service.get_by_open_id(&friend_id_str).await {
                Ok(user) => {
                    info!("通过 open_id 找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
                    user.id
                },
                Err(ErrorCode::NotFound) => {
                    // 尝试通过用户名查找
                    match user_service.get_by_name(&friend_id_str).await {
                        Ok(user) => {
                            info!("通过用户名找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
                            user.id
                        },
                        Err(ErrorCode::NotFound) => {
                            warn!("无法找到用户: {}", friend_id_str);
                            return Err((
                                StatusCode::NOT_FOUND,
                                Json(ErrorResponse::new(ErrorCode::NotFound, "用户不存在")),
                            ));
                        },
                        Err(code) => {
                            warn!("查询用户时发生错误: name={}, error={:?}", friend_id_str, code);
                            return Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse::new(code, "查询用户失败")),
                            ));
                        }
                    }
                },
                Err(code) => {
                    warn!("查询用户时发生错误: open_id={}, error={:?}", friend_id_str, code);
                    return Err((
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(ErrorResponse::new(code, "查询用户失败")),
                    ));
                }
            }
        }
    };
    
    info!("解析后的好友ID: user_id={}, friend_id={}", user_id, friend_id);

    if user_id == friend_id {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "不能添加自己为好友")),
        ));
    }

    let friend_service = FriendService::new(pool);
    match friend_service.add_friend(user_id, friend_id).await {
        Ok(_) => {
            info!("成功添加好友: user_id={}, friend_id={}", user_id, friend_id);
            Ok(Json(serde_json::json!({"status": "ok", "message": "好友添加成功"})))
        }
        Err(ErrorCode::InvalidInput) => {
            warn!("添加好友失败: 已经是好友或请求已存在");
            Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(ErrorCode::InvalidInput, "已经是好友或请求已存在")),
            ))
        }
        Err(code) => {
            warn!("添加好友失败: user_id={}, friend_id={}, error={:?}", user_id, friend_id, code);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(code, "添加好友失败")),
            ))
        }
    }
}

/// 获取好友列表
pub async fn get_friends(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
) -> impl IntoResponse {
    info!("获取好友列表: user_id={}", user_id);
    
    let friend_service = FriendService::new(pool);
    match friend_service.get_friends(user_id).await {
        Ok(friends) => {
            info!("获取好友列表成功: user_id={}, count={}", user_id, friends.len());
            Ok(Json(serde_json::json!({"friends": friends})))
        }
        Err(code) => {
            warn!("获取好友列表失败: user_id={}, error={:?}", user_id, code);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(code, "获取好友列表失败")),
            ))
        }
    }
}

/// 删除好友
pub async fn remove_friend(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_id): Extension<u64>,
    Path(friend_id_str): Path<String>,
) -> impl IntoResponse {
    info!("删除好友请求: user_id={}, friend_id={}", user_id, friend_id_str);
    
    let friend_id: u64 = match friend_id_str.parse() {
        Ok(id) => id,
        Err(_) => {
            // 如果不是数字，尝试通过 open_id（字符串）或用户名查找
            let user_service = UserService::new(pool.clone());
            match user_service.get_by_name(&friend_id_str).await {
                Ok(user) => user.id,
                Err(_) => {
                    // 尝试通过 open_id 查找（如果是数字）
                    if let Ok(open_id_number) = friend_id_str.parse::<u64>() {
                        let open_id = open_id_number.to_string();
                        match user_service.get_by_open_id(&open_id).await {
                            Ok(user) => user.id,
                            Err(_) => {
                                return Err((
                                    StatusCode::NOT_FOUND,
                                    Json(ErrorResponse::new(ErrorCode::NotFound, "用户不存在")),
                                ));
                            }
                        }
                    } else {
                        return Err((
                            StatusCode::BAD_REQUEST,
                            Json(ErrorResponse::new(ErrorCode::InvalidInput, "无效的用户ID或用户名")),
                        ));
                    }
                }
            }
        }
    };

    let friend_service = FriendService::new(pool);
    match friend_service.remove_friend(user_id, friend_id).await {
        Ok(_) => {
            info!("成功删除好友: user_id={}, friend_id={}", user_id, friend_id);
            Ok(Json(serde_json::json!({"status": "ok", "message": "好友删除成功"})))
        }
        Err(code) => {
            warn!("删除好友失败: user_id={}, friend_id={}, error={:?}", user_id, friend_id, code);
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(code, "删除好友失败")),
            ))
        }
    }
}

