use dotenvy::dotenv;
use tracing::info;
use tracing_subscriber;
use axum::serve;
use tokio::net::TcpListener;
use std::net::SocketAddr;
use axum::http::{header, Method};
use tower_http::cors::{CorsLayer, Any};

mod handlers;
mod routes;
mod config;

use crate::config::AppConfig;

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let cfg = AppConfig::load();
    
    // 创建 Redis 客户端
    use im_share::RedisConfig;
    let redis_config = RedisConfig::new(
        cfg.redis.host.clone(),
        cfg.redis.port,
        cfg.redis.db,
        cfg.redis.password.clone(),
    );
    let redis_client = std::sync::Arc::new(
        im_share::RedisClient::new(&redis_config)
            .await
            .expect("Redis 连接失败")
    );
    
    // 传递 MQTT 连接信息，每个用户会创建自己的 MQTT 客户端
    let mqtt_info = handlers::websocket::MqttConnectionInfo {
        host: cfg.mqtt.host.clone(),
        port: cfg.mqtt.port,
    };

    let app = routes::create_routes(mqtt_info, redis_client, cfg.jwt.clone())
        .layer(
            CorsLayer::new()
                .allow_origin(Any) // 开发时允许所有来源，生产环境应限制为特定域名
                .allow_methods(vec![Method::GET, Method::POST, Method::OPTIONS])
                .allow_headers(vec![header::CONTENT_TYPE, header::AUTHORIZATION]),
                // 注意：不能同时使用 allow_credentials(true) 和 allow_origin(Any)
                // 如果需要凭证，必须指定具体的来源，例如：
                // .allow_origin("http://localhost:1420".parse::<HeaderValue>().unwrap())
                // .allow_credentials(true)
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.connect.port));
    let listener = TcpListener::bind(addr).await.unwrap();
    
    info!(%addr, "im-connect WebSocket 监听启动");
    
    serve(listener, app.into_make_service())
        .await
        .unwrap();
}
