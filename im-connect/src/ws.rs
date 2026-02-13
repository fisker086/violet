//! WebSocket 处理器：握手、消息收发、心跳、登录注册

use crate::channel::UserChannelMap;
use crate::constants;
use crate::message;
use crate::redis::RedisConnection;
use axum::{
    extract::{ws::WebSocket, ws::WebSocketUpgrade, State},
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, error, warn};

/// WebSocket 应用状态
#[derive(Clone)]
pub struct AppState {
    pub channel_map: Arc<UserChannelMap>,
    pub redis: Arc<RedisConnection>,
    pub jwt_secret: String,
    pub broker_id: String,
    #[allow(dead_code)]
    pub heart_beat_time_ms: u64,
    pub timeout_ms: u64,
}

/// WebSocket 处理器
pub async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();

    // 发送任务
    let send_task = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if sender.send(axum::extract::ws::Message::Binary(msg)).await.is_err() {
                break;
            }
        }
    });

    // 接收任务
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            match msg {
                axum::extract::ws::Message::Binary(data) => {
                    if let Err(e) = handle_message(&data, &state, &tx).await {
                        error!("处理消息失败: {}", e);
                        break;
                    }
                }
                axum::extract::ws::Message::Close(_) => {
                    debug!("WebSocket 关闭");
                    break;
                }
                _ => {}
            }
        }
    });

    tokio::select! {
        _ = send_task => {}
        _ = recv_task => {}
    }
}

async fn handle_message(
    data: &[u8],
    state: &AppState,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let wrap = message::decode_wrap_from_bytes(data)?;
    let code = wrap.code;

    match code {
        constants::code::REGISTER => {
            // 登录注册逻辑
            if let Err(e) = crate::login::handle_register(&wrap, state, tx).await {
                error!("处理注册消息失败: {}", e);
                let error_msg = message::wrap_to_proto_bytes(
                    constants::code::ERROR,
                    None,
                    Some(&format!("登录失败: {}", e)),
                    None,
                );
                let _ = tx.send(error_msg);
            }
        }
        constants::code::HEART_BEAT => {
            // 心跳响应
            let response = message::wrap_to_proto_bytes(
                constants::code::HEART_BEAT_SUCCESS,
                None,
                None,
                None,
            );
            tx.send(response)?;
        }
        _ => {
            warn!("未知消息类型: code={}", code);
        }
    }

    Ok(())
}
