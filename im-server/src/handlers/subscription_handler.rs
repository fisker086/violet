use axum::{extract::{State, Extension, Path}, http::StatusCode, response::IntoResponse, Json};
use serde_json::json;
use std::sync::Arc;
use sqlx::MySqlPool;
use tracing::{info, warn, error};
use crate::{
    error::{ErrorCode, ErrorResponse},
    service::{SubscriptionService, UserService},
};

/// 根据订阅 ID 获取用户 ID（返回 open_id 的数字形式用于MQTT）
pub async fn get_user_id_by_subscription(
    State(subscription_service): State<Arc<SubscriptionService>>,
    Extension(pool): Extension<MySqlPool>,
    Path(subscription_id): Path<String>,
) -> impl IntoResponse {
    info!(subscription_id = %subscription_id, "查询订阅 ID 对应的用户");
    
    // 先从数据库查询订阅 ID
    let user_db_id = match sqlx::query_scalar::<_, u64>(
        "SELECT user_id FROM subscriptions WHERE subscription_id = ?"
    )
    .bind(&subscription_id)
    .fetch_optional(&pool)
    .await
    {
        Ok(Some(id)) => {
            info!(subscription_id = %subscription_id, user_id = %id, "从数据库找到订阅 ID");
            id
        },
        Ok(None) => {
            warn!(subscription_id = %subscription_id, "数据库中未找到订阅 ID，尝试从内存查询");
            // 如果数据库中没有，尝试从内存中查询（向后兼容）
            match subscription_service.get_user_id(&subscription_id) {
                Some(id) => {
                    warn!(subscription_id = %subscription_id, user_id = %id, "从内存中找到订阅 ID（未持久化）");
                    id
                },
                None => {
                    error!(subscription_id = %subscription_id, "订阅 ID 不存在（数据库和内存中都没有）");
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse::new(
                            ErrorCode::NotFound,
                            "订阅 ID 不存在",
                        )),
                    ));
                }
            }
        },
        Err(e) => {
            error!(subscription_id = %subscription_id, error = %e, "查询订阅 ID 失败");
            // 如果数据库查询失败，尝试从内存中查询（向后兼容）
            match subscription_service.get_user_id(&subscription_id) {
                Some(id) => {
                    warn!(subscription_id = %subscription_id, user_id = %id, "数据库查询失败，从内存中找到订阅 ID");
                    id
                },
                None => {
                    error!(subscription_id = %subscription_id, "订阅 ID 不存在（数据库查询失败且内存中也没有）");
                    return Err((
                        StatusCode::NOT_FOUND,
                        Json(ErrorResponse::new(
                            ErrorCode::NotFound,
                            "订阅 ID 不存在",
                        )),
                    ));
                }
            }
        }
    };
    
    // 根据数据库id查询用户，获取 open_id 的数字形式（用于MQTT）
    let user_service = UserService::new(pool);
    match user_service.get_by_id(user_db_id).await {
        Ok(user) => {
            let mqtt_id = user.get_mqtt_id();
            // 使用真正的 open_id（数据库字段），而不是 get_external_id()（可能返回用户名）
            let open_id = user.open_id.clone()
                .unwrap_or_else(|| user.id.to_string());
            info!(subscription_id = %subscription_id, user_id = %user_db_id, open_id = %open_id, mqtt_id = %mqtt_id, "成功获取用户信息");
            Ok(Json(json!({
                "user_id": user_db_id, // 保持向后兼容，返回数据库id
                "snowflake_id": mqtt_id, // 返回 open_id 的数字形式用于MQTT（保持字段名兼容）
                "open_id": open_id, // 返回真正的 open_id 字符串（数据库字段）
                "subscription_id": subscription_id,
            })))
        },
        Err(e) => {
            error!(subscription_id = %subscription_id, user_id = %user_db_id, error = ?e, "查询用户信息失败");
            Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(
                    ErrorCode::NotFound,
                    "用户不存在",
                )),
            ))
        },
    }
}

