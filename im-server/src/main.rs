use axum::Router;
use tower_http::{cors::{CorsLayer, Any}, trace::TraceLayer, limit::RequestBodyLimitLayer};
use tokio::net::TcpListener;
use std::net::SocketAddr;
use axum::http::{header, Method};
use dotenvy::dotenv;
use uuid::Uuid;
mod error;
mod model;
mod service;
mod middleware;
mod db;
mod handlers;
mod routes;
mod mqtt;
mod dto;
mod config;
mod redis;

use crate::{
    mqtt::MqttPublisher,
    config::AppConfig,
    service::SubscriptionService,
};
use std::sync::Arc;


#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_string()))
        .init();

    let cfg = AppConfig::load();
    
    // 初始化上传目录
    if let Err(e) = crate::handlers::upload_handler::init_upload_dir(&cfg.upload.path) {
        tracing::error!(error = %e, "初始化上传目录失败");
    } else {
        tracing::info!(path = %cfg.upload.path, "上传目录已初始化");
    }
    
    // 创建数据库连接池
    let pool = crate::db::create_pool(&cfg)
        .await
        .expect("数据库连接失败");
    
    // 创建 Redis 客户端
    use im_share::RedisConfig;
    let redis_config = RedisConfig::new(
        cfg.redis.host.clone(),
        cfg.redis.port,
        cfg.redis.db,
        cfg.redis.password.clone(),
    );
    let redis_client = Arc::new(
        im_share::RedisClient::new(&redis_config)
            .await
            .expect("Redis 连接失败")
    );
    
    // 创建 MQTT 发布器
    let publisher = MqttPublisher::new(
        &cfg.mqtt.host,
        cfg.mqtt.port,
        &format!("im-server-{}", Uuid::new_v4()),
    );

    // 创建订阅 ID 管理服务
    let subscription_service = Arc::new(SubscriptionService::new());

    // 创建路由
    let public_routes = crate::routes::create_public_routes(
        pool.clone(), 
        cfg.jwt.clone(),
        cfg.upload.clone(),
        subscription_service.clone(),
        redis_client.clone(),
    );
    let protected_routes = crate::routes::create_protected_routes(
        pool.clone(), 
        cfg.jwt.clone(),
        cfg.upload.clone(),
        cfg.srs.clone(),
        publisher,
        subscription_service.clone(),
        redis_client.clone(),
    );

    // 组装应用，先合并路由再添加 /api 前缀
    // 注意：public_routes 必须在 protected_routes 之前，避免认证中间件拦截公开路由
    let app = Router::new()
        .nest("/api", public_routes.merge(protected_routes))
        .layer(TraceLayer::new_for_http())
        .layer(RequestBodyLimitLayer::new(2 * 1024 * 1024)) // 允许最大 2MB 的请求体
        .layer(
            CorsLayer::new()
                .allow_origin(Any) // 开发时用 Any，生产时换成具体域名
                .allow_methods(vec![Method::GET, Method::POST, Method::DELETE, Method::PUT, Method::PATCH])
                .allow_headers(vec![header::CONTENT_TYPE, header::AUTHORIZATION]),
        );

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.server.port));
    let listener = TcpListener::bind(&addr).await.unwrap();

    // 打印所有注册的路由
    crate::routes::print_routes();
    
    tracing::info!("API 运行在 http://{}", addr);

    axum::serve(listener, app.into_make_service())
        .await
        .unwrap();
}