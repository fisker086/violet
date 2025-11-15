use axum::{extract::{Path, Extension}, http::StatusCode, response::IntoResponse, Json};
use sqlx::MySqlPool;
use serde::Deserialize;
use crate::{
    error::ErrorResponse,
    service::ImChatService,
    middleware::auth::UserIdentity,
};

#[derive(Deserialize)]
pub struct CreateChatRequest {
    pub chat_id: String,
    pub chat_type: i32,
    pub to_id: String,
}

#[derive(Deserialize)]
pub struct UpdateChatRequest {
    pub is_top: Option<i16>,
    pub is_mute: Option<i16>,
}

pub async fn get_or_create_chat(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<UserIdentity>,
    Json(req): Json<CreateChatRequest>,
) -> impl IntoResponse {
    let service = ImChatService::new(pool);
    
    // 使用 open_id 作为 owner_id（与创建聊天记录时保持一致）
    match service.get_or_create_chat(req.chat_id, req.chat_type, user_identity.get_external_id(), req.to_id).await {
        Ok(chat) => Ok(Json(chat)),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "创建或获取聊天失败")),
        )),
    }
}

pub async fn get_user_chats(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<UserIdentity>,
) -> impl IntoResponse {
    let service = ImChatService::new(pool);
    
    // 使用 open_id 查询聊天记录（与创建聊天记录时保持一致）
    // 使用包含名称信息的方法
    match service.get_user_chats_with_names(&user_identity.get_external_id()).await {
        Ok(chats) => Ok(Json(serde_json::json!({"chats": chats}))),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取聊天列表失败")),
        )),
    }
}

pub async fn update_chat(
    Extension(pool): Extension<MySqlPool>,
    Path(chat_id): Path<String>,
    Json(req): Json<UpdateChatRequest>,
) -> impl IntoResponse {
    let service = ImChatService::new(pool);
    
    if let Some(is_top) = req.is_top {
        match service.set_chat_top(&chat_id, is_top).await {
            Ok(_) => return Ok(Json(serde_json::json!({"status": "ok"}))),
            Err(e) => return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, "更新聊天失败")),
            )),
        }
    }
    
    if let Some(is_mute) = req.is_mute {
        match service.set_chat_mute(&chat_id, is_mute).await {
            Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
            Err(e) => Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorResponse::new(e, "更新聊天失败")),
            )),
        }
    } else {
        Ok(Json(serde_json::json!({"status": "ok"})))
    }
}

pub async fn delete_chat(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<UserIdentity>,
    Path(chat_id): Path<String>,
) -> impl IntoResponse {
    let service = ImChatService::new(pool);
    
    // 使用 open_id 删除聊天记录（与创建聊天记录时保持一致）
    match service.delete_chat(&chat_id, &user_identity.get_external_id()).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "删除聊天失败")),
        )),
    }
}

/// 获取未读消息统计
pub async fn get_unread_stats(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<UserIdentity>,
) -> impl IntoResponse {
    let service = ImChatService::new(pool);
    
    // 使用 open_id 查询未读消息统计
    match service.get_unread_message_stats(&user_identity.get_external_id()).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(e, "获取未读消息统计失败")),
        )),
    }
}

#[derive(Deserialize)]
pub struct UpdateReadSequenceRequest {
    pub read_sequence: i64,
}

/// 更新已读序列号
pub async fn update_read_sequence(
    Extension(pool): Extension<MySqlPool>,
    Path(chat_id): Path<String>,
    Json(req): Json<UpdateReadSequenceRequest>,
) -> impl IntoResponse {
    let service = ImChatService::new(pool);
    
    match service.update_read_sequence(&chat_id, req.read_sequence).await {
        Ok(_) => Ok(Json(serde_json::json!({"status": "ok"}))),
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "更新已读序列号失败")),
        )),
    }
}

#[derive(Deserialize)]
pub struct UpdateChatRemarkRequest {
    pub remark: Option<String>,
}

/// 更新群聊备注
pub async fn update_chat_remark(
    Extension(pool): Extension<MySqlPool>,
    Extension(user_identity): Extension<UserIdentity>,
    Path(chat_id): Path<String>,
    Json(req): Json<UpdateChatRemarkRequest>,
) -> impl IntoResponse {
    use tracing::info;
    let service = ImChatService::new(pool);
    
    info!("更新群聊备注请求: chat_id={}, owner_id={}, remark={:?}", 
          chat_id, user_identity.get_external_id(), req.remark);
    
    match service.update_chat_remark(&chat_id, &user_identity.get_external_id(), req.remark.clone()).await {
        Ok(_) => {
            info!("成功更新群聊备注: chat_id={}, remark={:?}", chat_id, req.remark);
            Ok(Json(serde_json::json!({"status": "ok", "message": "备注已更新"})))
        },
        Err(e) => Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(e, "更新群聊备注失败")),
        )),
    }
}

