use axum::{extract::{Extension, State}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use std::sync::Arc;
use uuid::Uuid;
use tracing::{info, error};
use crate::{
    dto::{LoginReq, LoginResponse},
    error::{ErrorCode, ErrorResponse},
    service::{UserService, SubscriptionService},
};
use im_share::{JwtSettings, generate_token_with_open_id};

/// 从数据库获取或创建订阅 ID
async fn get_or_create_subscription_from_db(pool: &MySqlPool, user_id: u64) -> Result<String, sqlx::Error> {
    // 先尝试从数据库获取现有的订阅 ID（只查询最近24小时内创建的订阅，过滤掉已不在线的用户）
    let existing = sqlx::query_scalar::<_, String>(
        "SELECT subscription_id FROM subscriptions 
         WHERE user_id = ? 
         AND created_at >= DATE_SUB(NOW(), INTERVAL 24 HOUR)
         ORDER BY created_at DESC LIMIT 1"
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    
    if let Some(sub_id) = existing {
        info!(user_id = %user_id, subscription_id = %sub_id, "从数据库获取现有订阅 ID");
        return Ok(sub_id);
    }
    
    // 如果不存在，创建新的订阅 ID
    let subscription_id = format!("sub_{}", Uuid::new_v4().to_string().replace("-", ""));
    
    info!(user_id = %user_id, subscription_id = %subscription_id, "创建新的订阅 ID");
    
    let result = sqlx::query(
        "INSERT INTO subscriptions (subscription_id, user_id) VALUES (?, ?)
         ON DUPLICATE KEY UPDATE subscription_id = subscription_id"
    )
    .bind(&subscription_id)
    .bind(user_id)
    .execute(pool)
    .await;
    
    match result {
        Ok(_) => {
            info!(user_id = %user_id, subscription_id = %subscription_id, "订阅 ID 已保存到数据库");
            Ok(subscription_id)
        },
        Err(e) => {
            error!(user_id = %user_id, subscription_id = %subscription_id, error = %e, "保存订阅 ID 到数据库失败");
            Err(e)
        }
    }
}

pub async fn login(
    Extension(pool): Extension<MySqlPool>,
    Extension(jwt_cfg): Extension<JwtSettings>,
    State(subscription_service): State<Arc<SubscriptionService>>,
    Json(payload): Json<LoginReq>,
) -> impl IntoResponse {
    let user_service = UserService::new(pool.clone());

    match user_service
        .verify_password(&payload.username, &payload.password)
        .await
    {
        Ok(user) => {
            // 确保用户有 open_id
            let open_id = if user.open_id.is_none() {
                match user_service.ensure_open_id(user.id).await {
                    Ok(oid) => {
                        info!(user_id = %user.id, open_id = %oid, "为用户生成 open_id");
                        oid
                    },
                    Err(e) => {
                        error!(user_id = %user.id, error = %e, "生成 open_id 失败");
                        return Err((
                            StatusCode::INTERNAL_SERVER_ERROR,
                            Json(ErrorResponse::new(
                                ErrorCode::Internal,
                                "生成用户标识失败，请稍后重试",
                            )),
                        ));
                    }
                }
            } else {
                user.open_id.as_ref().unwrap().clone()
            };
            
            // 从 open_id 解析数字（用于 JWT）
            let open_id_number = open_id.parse::<u64>()
                .map_err(|_| {
                    error!(user_id = %user.id, open_id = %open_id, "open_id 不是数字格式，无法生成 token");
                    return (StatusCode::INTERNAL_SERVER_ERROR, Json(ErrorResponse::new(ErrorCode::Internal, "用户 open_id 格式错误")));
                })?;
            
            match generate_token_with_open_id(open_id_number, &jwt_cfg) {
                Ok(token) => {
                    // 生成或获取订阅 ID（必须保存到数据库）
                    let subscription_id = match get_or_create_subscription_from_db(&pool, user.id).await {
                        Ok(sub_id) => {
                            info!(user_id = %user.id, open_id = %open_id, subscription_id = %sub_id, "登录成功，订阅 ID 已从数据库获取或创建");
                            // 同步到内存中的订阅服务（用于快速查询）
                            subscription_service.add_subscription_id(sub_id.clone(), user.id);
                            sub_id
                        },
                        Err(e) => {
                            error!(user_id = %user.id, open_id = %open_id, error = %e, "从数据库获取或创建订阅 ID 失败");
                            // 数据库操作失败，返回错误
                            return Err((
                                StatusCode::INTERNAL_SERVER_ERROR,
                                Json(ErrorResponse::new(
                                    ErrorCode::Internal,
                                    "创建订阅 ID 失败，请稍后重试",
                                )),
                            ));
                        }
                    };
                    
                    info!(user_id = %user.id, open_id = %open_id, subscription_id = %subscription_id, "返回登录响应");
                    Ok(Json(LoginResponse { 
                        token, 
                        user,
                        subscription_id,
                    }))
                },
                Err(_) => Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(ErrorCode::Internal, "生成 token 失败")),
                )),
            }
        }
        Err(code) => {
            let status = match code {
                ErrorCode::Unauthorized => StatusCode::UNAUTHORIZED,
                ErrorCode::NotFound => StatusCode::NOT_FOUND,
                _ => StatusCode::INTERNAL_SERVER_ERROR,
            };
            let msg = match code {
                ErrorCode::Unauthorized => "用户名或密码错误",
                ErrorCode::NotFound => "用户不存在",
                _ => "服务器内部错误",
            };
            Err((status, Json(ErrorResponse::new(code, msg))))
        }
    }
}

