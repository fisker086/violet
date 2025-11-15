use axum::{extract::{Path, Extension}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::{
    error::ErrorResponse,
    service::ImUserService,
    model::{ImUser, ImUserData},
    redis::RedisClient,
};

#[derive(Deserialize)]
pub struct CreateImUserRequest {
    pub user_id: String,
    pub user_name: String,
    pub password: String,
    pub mobile: Option<String>,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub user_name: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct LoginResponse {
    pub user: ImUser,
    pub token: String,
}

pub async fn create_user(
    Extension(pool): Extension<MySqlPool>,
    Extension(redis_client): Extension<Arc<RedisClient>>,
    Json(req): Json<CreateImUserRequest>,
) -> impl IntoResponse {
    let service = ImUserService::with_redis(pool, redis_client);
    
    match service.create(req.user_id, req.user_name, req.password, req.mobile).await {
        Ok(user) => Ok(Json(user)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "创建用户失败")),
        )),
    }
}

pub async fn login(
    Extension(pool): Extension<MySqlPool>,
    Json(req): Json<LoginRequest>,
) -> impl IntoResponse {
    let service = ImUserService::new(pool);
    
    match service.verify_password(&req.user_name, &req.password).await {
        Ok(user) => {
            // TODO: 生成JWT token
            let token = "dummy_token".to_string();
            Ok(Json(LoginResponse { user, token }))
        }
        Err(e) => Err((
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse::new(e, "登录失败")),
        )),
    }
}

pub async fn get_user(
    Path(user_id): Path<String>,
    Extension(pool): Extension<MySqlPool>,
) -> impl IntoResponse {
    let service = ImUserService::new(pool);
    
    match service.get_by_user_id(&user_id).await {
        Ok(user) => Ok(Json(user)),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(e, "用户不存在")),
        )),
    }
}

pub async fn get_user_data(
    Path(user_id): Path<String>,
    Extension(pool): Extension<MySqlPool>,
) -> impl IntoResponse {
    let service = ImUserService::new(pool);
    
    match service.get_user_data(&user_id).await {
        Ok(user_data) => Ok(Json(user_data)),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(e, "用户数据不存在")),
        )),
    }
}

pub async fn upsert_user_data(
    Extension(pool): Extension<MySqlPool>,
    Json(user_data): Json<ImUserData>,
) -> impl IntoResponse {
    let service = ImUserService::new(pool);
    
    match service.upsert_user_data(user_data).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "更新用户数据失败")),
        )),
    }
}

