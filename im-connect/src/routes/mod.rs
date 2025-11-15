use axum::{Router, routing::get, Extension};
use std::sync::Arc;
use im_share::{RedisClient, JwtSettings};
use crate::handlers::websocket;

pub fn create_routes(mqtt_info: websocket::MqttConnectionInfo, redis_client: Arc<RedisClient>, jwt_cfg: JwtSettings) -> Router {
    Router::new()
        .route("/ws/{subscription_id}", get(websocket::ws_handler))
        .layer(Extension(redis_client))
        .layer(Extension(jwt_cfg))
        .with_state(mqtt_info)
}

