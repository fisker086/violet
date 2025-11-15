use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use sqlx::MySqlPool;
use std::sync::Arc;
use tracing::{warn, error};
use crate::{
    service::UserService,
    error::ErrorCode,
    redis::RedisClient,
};
use im_share::{verify_token, JwtSettings};

/// 用户标识信息（用于在请求扩展中传递）
#[derive(Clone, Debug)]
pub struct UserIdentity {
    /// 数据库 id（用于内部查询）
    pub db_id: u64,
    /// Open ID（用于外部标识，唯一标识符）
    pub open_id: String,
}

impl UserIdentity {
    /// 获取用于 MQTT 的 ID（从 open_id 解析，如果是数字字符串则解析，否则使用数据库 id）
    #[allow(dead_code)]
    pub fn get_mqtt_id(&self) -> u64 {
        // 如果 open_id 是数字字符串（雪花算法生成的），解析它
        if let Ok(id) = self.open_id.parse::<u64>() {
            return id;
        }
        // 否则使用数据库 id
        self.db_id
    }
    
    /// 获取用于外部标识的 ID（open_id）
    pub fn get_external_id(&self) -> String {
        self.open_id.clone()
    }
}

pub async fn auth_middleware(
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // 从请求头获取 token
    let token = request
        .headers()
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
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // 从扩展中获取 JWT 配置和数据库连接池
    let jwt_cfg = request
        .extensions()
        .get::<JwtSettings>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let pool = request
        .extensions()
        .get::<MySqlPool>()
        .cloned()
        .ok_or(StatusCode::INTERNAL_SERVER_ERROR)?;
    
    let redis_client = request
        .extensions()
        .get::<Arc<RedisClient>>()
        .cloned();

    // 验证 token
    let claims = verify_token(&token, &jwt_cfg)
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    // 根据 token 中的 user_id 类型，查询用户信息
    // 如果 Redis 可用，使用带缓存的 UserService
    let user_service = if let Some(redis) = redis_client {
        UserService::with_redis(pool, redis)
    } else {
        UserService::new(pool)
    };
    let user = if claims.is_open_id {
        // Token 中包含的是 open_id 的数字形式（雪花算法生成的）
        // 将数字转换为字符串查询
        let open_id = claims.user_id.to_string();
        match user_service.get_by_open_id(&open_id).await {
            Ok(u) => u,
            Err(ErrorCode::NotFound) => {
                warn!(open_id = %open_id, "Token 中的 open_id 对应的用户不存在");
                return Err(StatusCode::UNAUTHORIZED);
            },
            Err(_) => {
                error!(open_id = %open_id, "查询用户失败");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        // Token 中包含的是数据库 id（向后兼容旧 token）
        // 先尝试按数据库 ID 查询
        match user_service.get_by_id(claims.user_id).await {
            Ok(u) => u,
            Err(ErrorCode::NotFound) => {
                // 如果按数据库 ID 查询失败，且 ID 看起来像 snowflake ID（大于 10^9），
                // 尝试作为 open_id 查询（可能是旧 token 错误标记了 is_open_id）
                if claims.user_id > 1_000_000_000 {
                    let open_id = claims.user_id.to_string();
                    warn!(user_id = %claims.user_id, open_id = %open_id, "按数据库 ID 查询失败，尝试作为 open_id 查询");
                    match user_service.get_by_open_id(&open_id).await {
                        Ok(u) => u,
                        Err(ErrorCode::NotFound) => {
                            warn!(user_id = %claims.user_id, open_id = %open_id, "Token 中的 user_id/open_id 对应的用户不存在");
                            return Err(StatusCode::UNAUTHORIZED);
                        },
                        Err(_) => {
                            error!(user_id = %claims.user_id, open_id = %open_id, "查询用户失败");
                            return Err(StatusCode::INTERNAL_SERVER_ERROR);
                        }
                    }
                } else {
                    warn!(user_id = %claims.user_id, "Token 中的 user_id 对应的用户不存在");
                    return Err(StatusCode::UNAUTHORIZED);
                }
            },
            Err(_) => {
                error!(user_id = %claims.user_id, "查询用户失败");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    };
    
    // 确保用户有 open_id
    let open_id = if user.open_id.is_none() {
        match user_service.ensure_open_id(user.id).await {
            Ok(oid) => {
                warn!(user_id = %user.id, open_id = %oid, "为用户生成 open_id（在认证中间件中）");
                oid
            },
            Err(_) => {
                error!(user_id = %user.id, "生成 open_id 失败");
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }
        }
    } else {
        user.open_id.unwrap()
    };
    
    // 创建用户标识信息
    let user_identity = UserIdentity {
        db_id: user.id,
        open_id,
    };
    
    // 将用户标识信息添加到请求扩展中，供后续处理程序使用
    // 为了向后兼容，也添加数据库 id
    request.extensions_mut().insert(user_identity.clone());
    request.extensions_mut().insert(user_identity.db_id); // 向后兼容

    Ok(next.run(request).await)
}

