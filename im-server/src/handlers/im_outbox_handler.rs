use axum::{extract::{Path, Extension, Query}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use serde::Deserialize;
use std::collections::HashMap;
use crate::{
    error::ErrorResponse,
    service::ImOutboxService,
};

#[derive(Deserialize)]
pub struct CreateOutboxRequest {
    pub message_id: String,
    pub payload: String,
    pub exchange: String,
    pub routing_key: String,
}

#[derive(Deserialize)]
pub struct UpdateOutboxStatusRequest {
    pub status: String,
}

#[derive(Deserialize)]
pub struct SetNextTryRequest {
    #[allow(dead_code)]
    pub next_try_at: i64,
}

pub async fn create_outbox(
    Extension(pool): Extension<MySqlPool>,
    Json(req): Json<CreateOutboxRequest>,
) -> impl IntoResponse {
    let service = ImOutboxService::new(pool);
    
    match service.create(&req.message_id, &req.payload, &req.exchange, &req.routing_key).await {
        Ok(outbox) => Ok(Json(outbox)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "创建发件箱记录失败")),
        )),
    }
}

pub async fn get_outbox(
    Extension(pool): Extension<MySqlPool>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let service = ImOutboxService::new(pool);
    
    match service.get_by_id(id).await {
        Ok(outbox) => Ok(Json(outbox)),
        Err(e) => Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(e, "发件箱记录不存在")),
        )),
    }
}

pub async fn update_outbox_status(
    Extension(pool): Extension<MySqlPool>,
    Path(id): Path<u64>,
    Json(req): Json<UpdateOutboxStatusRequest>,
) -> impl IntoResponse {
    let service = ImOutboxService::new(pool);
    
    match service.update_status(id, &req.status).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "更新发件箱状态失败")),
        )),
    }
}

pub async fn mark_sent(
    Extension(pool): Extension<MySqlPool>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    let service = ImOutboxService::new(pool);
    
    match service.mark_sent(id).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "标记已发送失败")),
        )),
    }
}

pub async fn get_pending_messages(
    Extension(pool): Extension<MySqlPool>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let service = ImOutboxService::new(pool);
    let limit = params.get("limit").and_then(|s| s.parse::<i32>().ok()).unwrap_or(100);
    
    match service.get_pending_messages(limit).await {
        Ok(messages) => Ok(Json(serde_json::json!({"messages": messages}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取待发送消息失败")),
        )),
    }
}

pub async fn get_failed_messages(
    Extension(pool): Extension<MySqlPool>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let service = ImOutboxService::new(pool);
    let limit = params.get("limit").and_then(|s| s.parse::<i32>().ok()).unwrap_or(100);
    
    match service.get_failed_messages(limit).await {
        Ok(messages) => Ok(Json(serde_json::json!({"messages": messages}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取失败消息失败")),
        )),
    }
}

