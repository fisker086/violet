use axum::{extract::{Path, Extension}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use tracing::{info, warn};
use serde::Deserialize;
use crate::{
    dto::CreateUserReq,
    error::{ErrorCode, ErrorResponse},
    service::UserService,
};

pub async fn get_user(
    Path(id): Path<String>,
    Extension(pool): Extension<MySqlPool>,
    Extension(_user_id): Extension<u64>, // 认证中间件注入的用户ID，这里不需要使用，但需要存在以通过认证
) -> impl IntoResponse {
    info!("查询用户，open_id或用户名: {} (请求来自用户ID: {})", id, _user_id);
    
    let user_service = UserService::new(pool);
    
    // 优先尝试通过 open_id 查询（UUID 格式）
    if id.len() == 36 && id.contains('-') {
        match user_service.get_by_open_id(&id).await {
            Ok(user) => {
                info!("通过open_id找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
                return Ok(Json(user));
            },
            Err(ErrorCode::NotFound) => {
                // 继续尝试其他方式
            },
            Err(code) => {
                warn!("通过open_id查询用户时发生错误: id={}, error={:?}", id, code);
                let status = match code {
                    ErrorCode::NotFound => StatusCode::NOT_FOUND,
                    ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                    ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                let msg = match code {
                    ErrorCode::NotFound => "用户不存在",
                    ErrorCode::InvalidInput => "请求参数错误",
                    ErrorCode::Database => "数据库查询错误",
                    _ => "服务器内部错误",
                };
                return Err((status, Json(ErrorResponse::new(code, msg))));
            }
        }
    }
    
    // 如果是数字，优先尝试作为雪花ID查询（不再尝试数据库ID）
    if let Ok(numeric_id) = id.parse::<u64>() {
        // 尝试通过 open_id 的数字形式获取用户（open_id 是雪花算法生成的数字字符串）
        let open_id = numeric_id.to_string();
        match user_service.get_by_open_id(&open_id).await {
            Ok(user) => {
                info!("通过 open_id 找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
                return Ok(Json(user));
            },
            Err(ErrorCode::NotFound) => {
                // 雪花ID未找到，继续尝试通过用户名查询（不再尝试数据库ID）
                warn!("通过雪花ID未找到用户: {}，继续尝试通过用户名查询", numeric_id);
            },
            Err(code) => {
                warn!("通过雪花ID查询用户时发生错误: id={}, error={:?}", numeric_id, code);
                let status = match code {
                    ErrorCode::NotFound => StatusCode::NOT_FOUND,
                    ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                    ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
                    _ => StatusCode::INTERNAL_SERVER_ERROR,
                };
                let msg = match code {
                    ErrorCode::NotFound => "用户不存在",
                    ErrorCode::InvalidInput => "请求参数错误",
                    ErrorCode::Database => "数据库查询错误",
                    _ => "服务器内部错误",
                };
                return Err((status, Json(ErrorResponse::new(code, msg))));
            }
        }
    }
    
    // 尝试通过用户名查询
    match user_service.get_by_name(&id).await {
        Ok(user) => {
            info!("通过用户名找到用户: id={}, open_id={:?}, name={}", user.id, user.open_id, user.name);
            Ok(Json(user))
        },
        Err(ErrorCode::NotFound) => {
            warn!("通过用户名也未找到用户: {}", id);
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(ErrorCode::NotFound, "用户不存在")),
            ))
        },
        Err(code) => {
            warn!("查询用户时发生错误: id={}, error={:?}", id, code);
            let status = match code {
                ErrorCode::NotFound => StatusCode::NOT_FOUND,
                ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::NotFound => "用户不存在",
                ErrorCode::InvalidInput => "请求参数错误",
                ErrorCode::Database => "数据库查询错误",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}

pub async fn get_current_user(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<crate::middleware::auth::UserIdentity>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool);
    // 使用 open_id 查询用户
    match user_service.get_by_open_id(&user_identity.open_id).await {
        Ok(mut user) => {
            // 确保用户有 open_id
            if user.open_id.is_none() {
                if let Ok(open_id) = user_service.ensure_open_id(user.id).await {
                    user.open_id = Some(open_id);
                }
            }
            Ok(Json(user))
        },
        Err(ErrorCode::NotFound) => {
            // 如果通过 open_id 找不到，尝试通过数据库 id（向后兼容）
            match user_service.get_by_id(user_identity.db_id).await {
                Ok(mut user) => {
                    // 确保用户有 open_id
                    if user.open_id.is_none() {
                        if let Ok(open_id) = user_service.ensure_open_id(user.id).await {
                            user.open_id = Some(open_id);
                        }
                    }
                    Ok(Json(user))
                },
                Err(code) => {
                    let status = match code {
                        ErrorCode::NotFound => StatusCode::NOT_FOUND,
                        _ => StatusCode::INTERNAL_SERVER_ERROR,
                    };
                    let msg = match code {
                        ErrorCode::NotFound => "用户不存在",
                        _ => "服务器内部错误",
                    };
                    Err((status, Json(ErrorResponse::new(code, msg))))
                }
            }
        },
        Err(code) => {
            let status = match code {
                ErrorCode::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::NotFound => "用户不存在",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}

pub async fn create_user(
    Extension(pool): Extension<MySqlPool>,
    Json(payload): Json<CreateUserReq>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool);
    match user_service
        .create(payload.name, payload.email, payload.password, payload.phone)
        .await
    {
        Ok(user) => Ok(Json(user)),
        Err(code) => {
            let status = match code {
                ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::InvalidInput => "用户名、邮箱或手机号格式错误或已被占用",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}

#[derive(Deserialize)]
pub struct UpdateUserReq {
    pub name: Option<String>,
    pub file_name: Option<String>,
    pub abstract_field: Option<String>,
    pub phone: Option<String>,
    pub gender: Option<i8>,
}

/// 更新当前用户信息
pub async fn update_current_user(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<crate::middleware::auth::UserIdentity>,
    Json(payload): Json<UpdateUserReq>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool);
    // 使用数据库 id 更新用户（因为 update_user 方法需要数据库 id）
    match user_service
        .update_user(
            user_identity.db_id,
            payload.name,
            payload.file_name,
            payload.abstract_field,
            payload.phone,
            payload.gender,
        )
        .await
    {
        Ok(user) => Ok(Json(user)),
        Err(code) => {
            let status = match code {
                ErrorCode::NotFound => StatusCode::NOT_FOUND,
                ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::NotFound => "用户不存在",
                ErrorCode::InvalidInput => "请求参数错误或昵称已被占用",
                ErrorCode::Database => "数据库更新错误",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}

/// 检查昵称是否可用（不需要认证）
/// 用于前端实时验证昵称是否已被占用
pub async fn check_name_available(
    Path(name): Path<String>,
    Extension(pool): Extension<MySqlPool>,
) -> Json<serde_json::Value> {
    use crate::service::UserService;
    
    // 基本验证
    if name.is_empty() {
        return Json(serde_json::json!({ 
            "available": false, 
            "message": "昵称不能为空" 
        }));
    }
    
    if name.len() < 2 || name.len() > 20 {
        return Json(serde_json::json!({ 
            "available": false, 
            "message": "昵称长度必须在2-20个字符之间" 
        }));
    }
    
    let user_service = UserService::new(pool);
    match user_service.get_by_name(&name).await {
        Ok(_) => {
            // 昵称已被占用
            Json(serde_json::json!({ 
                "available": false, 
                "message": "该昵称已被使用" 
            }))
        }
        Err(ErrorCode::NotFound) => {
            // 昵称可用
            Json(serde_json::json!({ 
                "available": true, 
                "message": "该昵称可以使用" 
            }))
        }
        Err(_) => {
            // 其他错误
            Json(serde_json::json!({ 
                "available": false, 
                "message": "检查失败，请稍后重试" 
            }))
        }
    }
}

/// 根据用户ID（open_id、雪花ID、数据库ID或用户名）获取用户名（不需要认证）
/// 用于内部服务（如 im-connect）获取用户名
pub async fn get_user_name(
    Path(id): Path<String>,
    Extension(pool): Extension<MySqlPool>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool);
    
    // 优先尝试通过 open_id 查询（UUID 格式）
    if id.len() == 36 && id.contains('-') {
        match user_service.get_by_open_id(&id).await {
            Ok(user) => {
                return Ok(Json(serde_json::json!({ "name": user.name })));
            },
            Err(ErrorCode::NotFound) => {
                // 继续尝试其他方式
            },
            Err(_) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(ErrorCode::Database, "数据库查询错误")),
                ));
            }
        }
    }
    
    // 如果是数字，尝试作为 open_id 查询
    if let Ok(numeric_id) = id.parse::<u64>() {
        // 尝试通过 open_id 的数字形式获取用户
        let open_id = numeric_id.to_string();
        match user_service.get_by_open_id(&open_id).await {
            Ok(user) => {
                return Ok(Json(serde_json::json!({ "name": user.name })));
            },
            Err(ErrorCode::NotFound) => {
                // 雪花ID未找到，继续尝试通过用户名查询（不再尝试数据库ID）
            },
            Err(_) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(ErrorCode::Database, "数据库查询错误")),
                ));
            }
        }
    }
    
    // 尝试通过用户名查询
    match user_service.get_by_name(&id).await {
        Ok(user) => {
            Ok(Json(serde_json::json!({ "name": user.name })))
        },
        Err(ErrorCode::NotFound) => {
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(ErrorCode::NotFound, "用户不存在")),
            ))
        },
        Err(code) => {
            let status = match code {
                ErrorCode::NotFound => StatusCode::NOT_FOUND,
                ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::NotFound => "用户不存在",
                ErrorCode::InvalidInput => "请求参数错误",
                ErrorCode::Database => "数据库查询错误",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}


/// 根据用户ID（open_id、数据库ID或用户名）获取 open_id 的数字形式（不需要认证）
/// 用于内部服务（如 im-connect）获取用户的数字ID（用于MQTT）
/// 返回 open_id 的数字形式（如果 open_id 是数字字符串）
pub async fn get_user_snowflake_id(
    Path(id): Path<String>,
    Extension(pool): Extension<MySqlPool>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool);
    
    // 如果已经是数字，尝试作为 open_id 查询
    if let Ok(open_id_number) = id.parse::<u64>() {
        let open_id = open_id_number.to_string();
        match user_service.get_by_open_id(&open_id).await {
            Ok(user) => {
                // 返回 open_id 的数字形式（用于MQTT兼容）
                if let Ok(mqtt_id) = user.get_external_id().parse::<u64>() {
                    return Ok(Json(serde_json::json!({ 
                        "snowflake_id": mqtt_id, // 保持字段名兼容
                        "open_id": user.get_external_id()
                    })));
                }
            },
            Err(ErrorCode::NotFound) => {
                // 继续尝试其他方式
            },
            Err(_) => {
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(ErrorCode::Database, "数据库查询错误")),
                ));
            }
        }
    }
    
    // 优先尝试通过 open_id 查询（UUID 格式或数字字符串）
    match user_service.get_by_open_id(&id).await {
        Ok(user) => {
            // 返回 open_id 的数字形式（如果 open_id 是数字字符串）
            if let Ok(mqtt_id) = user.get_external_id().parse::<u64>() {
                return Ok(Json(serde_json::json!({ 
                    "snowflake_id": mqtt_id, // 保持字段名兼容
                    "open_id": user.get_external_id()
                })));
            } else {
                // open_id 是 UUID 格式，无法转换为数字，返回错误
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(ErrorCode::InvalidInput, "用户的 open_id 不是数字格式，无法用于MQTT")),
                ));
            }
        },
        Err(ErrorCode::NotFound) => {
            // 继续尝试其他方式
        },
        Err(_) => {
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::Database, "数据库查询错误")),
            ));
        }
    }
    
    // 尝试通过用户名查询
    match user_service.get_by_name(&id).await {
        Ok(user) => {
            // 返回 open_id 的数字形式（如果 open_id 是数字字符串）
            if let Ok(mqtt_id) = user.get_external_id().parse::<u64>() {
                Ok(Json(serde_json::json!({ 
                    "snowflake_id": mqtt_id, // 保持字段名兼容
                    "open_id": user.get_external_id()
                })))
            } else {
                Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorResponse::new(ErrorCode::InvalidInput, "用户的 open_id 不是数字格式，无法用于MQTT")),
                ))
            }
        },
        Err(ErrorCode::NotFound) => {
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(ErrorCode::NotFound, "用户不存在")),
            ))
        },
        Err(code) => {
            let status = match code {
                ErrorCode::NotFound => StatusCode::NOT_FOUND,
                ErrorCode::InvalidInput => StatusCode::BAD_REQUEST,
                ErrorCode::Database => StatusCode::INTERNAL_SERVER_ERROR,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::NotFound => "用户不存在",
                ErrorCode::InvalidInput => "请求参数错误",
                ErrorCode::Database => "数据库查询错误",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}

