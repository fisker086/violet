//! im-connect：WebSocket 长连接网关（Rust 版）
//! - 鉴权：JWT（Query/Header/Cookie）
//! - 协议：二进制 Proto (im_message_wrap.proto)
//! - 注册发现：Nacos
//! - 用户会话：Redis (IM-USER-{userId})
//! - 消息队列：RabbitMQ，按 code 分发到连接

mod auth;
mod channel;
mod cli;
mod config;
mod constants;
mod device;
mod login;
mod message;
mod mq;
mod nacos;
mod proto;
mod redis;
mod ws;

use axum::{routing::get, Router};
use std::sync::Arc;
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cli = cli::Cli::parse();

    // 构建日志过滤器
    // 将 nacos_sdk 的健康检查相关警告降级，避免干扰日志
    // 这些警告通常发生在连接重连时，SDK 会自动重试，可以安全忽略
    let mut log_filter = cli.log_filter();
    
    // 添加 nacos_sdk 的过滤规则，将健康检查相关的警告降级为 DEBUG
    if !log_filter.contains("nacos_sdk") {
        let nacos_filter = if cli.nacos_register_enabled {
            // Nacos 注册启用时，将健康检查警告降级为 DEBUG（SDK 会自动重试）
            // "Connection is unregistered" 等警告是正常的重连过程，可以安全忽略
            "nacos_sdk::common::remote::grpc::message=debug,nacos_sdk::common::remote::grpc::nacos_grpc_connection=debug,nacos_sdk::common::remote::grpc::utils=warn,nacos_sdk=info"
        } else {
            // Nacos 注册未启用时，将所有 nacos_sdk 日志降级为 WARN
            "nacos_sdk::common::remote::grpc::message=warn,nacos_sdk::common::remote::grpc::nacos_grpc_connection=warn,nacos_sdk::common::remote::grpc::utils=warn,nacos_sdk=warn"
        };
        
        if log_filter.is_empty() {
            log_filter = nacos_filter.to_string();
        } else {
            log_filter = format!("{},{}", log_filter, nacos_filter);
        }
    }
    
    // 初始化日志
    tracing_subscriber::fmt()
        .with_env_filter(log_filter)
        .init();

    // 从命令行参数构建配置
    let cfg = config::AppConfig::from_cli(&cli);
    
    // Broker ID 优先级：命令行参数 > 环境变量 > 配置文件 > 自动生成
    let broker_id = cfg.broker_id().to_string();
    let broker_source = if cli.broker_id.is_some() {
        "命令行参数 --broker-id"
    } else if std::env::var("BROKER_ID").is_ok() {
        "环境变量 BROKER_ID"
    } else if cfg.broker_id.is_some() {
        "配置文件 broker_id"
    } else {
        "自动生成"
    };
    info!("broker_id = {} (来源: {})", broker_id, broker_source);
    
    // 显示 Nacos 配置信息
    info!("Nacos 配置: 服务器={}, 服务名={}, 分组={}, 命名空间={:?}, 注册启用={}", 
        cfg.nacos.server_addr, 
        cfg.nacos.service_name,
        cfg.nacos.group,
        cfg.nacos.namespace.as_deref().unwrap_or("默认"),
        cfg.nacos.register_enabled
    );
    if !cfg.nacos.register_enabled {
        info!("提示: Nacos 注册未启用，使用 --nacos-register-enabled 启用服务注册");
        info!("注意: 如果看到 nacos_sdk 相关的连接错误，可以安全忽略（Nacos 注册未启用时这些错误不影响应用功能）");
    }
    
    // 连接 Redis
    info!("正在连接 Redis: {}:{}", cfg.redis.host, cfg.redis.port);
    let redis_client = redis::get_redis_client(&cfg.redis)
        .map_err(|e| format!("Redis 客户端创建失败 ({}:{}) - {}", cfg.redis.host, cfg.redis.port, e))?;
    let redis_conn = redis_client.get_multiplexed_async_connection().await
        .map_err(|e| format!("Redis 连接失败 ({}:{}) - {}。请检查 Redis 服务是否已启动", cfg.redis.host, cfg.redis.port, e))?;
    info!("Redis 连接成功: {}:{}", cfg.redis.host, cfg.redis.port);
    let redis: Arc<_> = Arc::new(redis_conn);
    
    let channel_map = Arc::new(channel::UserChannelMap::new(
        broker_id.clone(),
        cfg.connect.multi_device_enabled,
    ));

    let state = ws::AppState {
        channel_map: channel_map.clone(),
        redis: redis.clone(),
        jwt_secret: cfg.jwt.secret.clone(),
        broker_id: broker_id.clone(),
        heart_beat_time_ms: cfg.connect.heart_beat_time_ms,
        timeout_ms: cfg.connect.timeout_ms,
    };

    let app = Router::new()
        .route(&cfg.connect.websocket.path, get(ws::ws_handler))
        .with_state(state);

    if cfg.connect.websocket.enable && !cfg.connect.websocket.ports.is_empty() {
        let ports = cfg.connect.websocket.ports.clone();
        let bind = cfg.server.bind.clone();
        let ws_path = cfg.connect.websocket.path.clone();

        info!("配置了 {} 个 WebSocket 监听端口: {:?} (用于负载均衡和高可用)", ports.len(), ports);
        
        let mut bound_ports = Vec::new();
        for port in ports.iter().copied() {
            let addr = config::ws_bind_addr(&bind, port);
            match tokio::net::TcpListener::bind(&addr).await {
                Ok(listener) => {
                    let app = app.clone();
                    let bind_clone = bind.clone();
                    let ws_path_clone = ws_path.clone();
                    tokio::spawn(async move {
                        info!("WebSocket 监听成功: {}:{} (路径: {})", bind_clone, port, ws_path_clone);
                        axum::serve(listener, app).await.ok();
                    });
                    bound_ports.push(port);
                }
                Err(e) => {
                    tracing::error!("WebSocket 端口绑定失败: {}:{} - {}。请检查端口是否被占用", bind, port, e);
                }
            }
        }
        
        if bound_ports.is_empty() {
            return Err("所有 WebSocket 端口绑定失败，无法启动服务".into());
        }
        
        info!("成功绑定 {} 个 WebSocket 端口: {:?}", bound_ports.len(), bound_ports);

        // Nacos 服务注册
        info!("Nacos 配置: 服务器={}, 服务名={}, 分组={}, 命名空间={:?}, 注册启用={}", 
            cfg.nacos.server_addr, 
            cfg.nacos.service_name,
            cfg.nacos.group,
            cfg.nacos.namespace.as_deref().unwrap_or("默认"),
            cfg.nacos.register_enabled
        );
        
        if cfg.nacos.register_enabled {
            info!("开始注册服务到 Nacos...");
            let local_ip = nacos::local_ip();
            info!("检测到本机 IP: {}", local_ip);
            match nacos::register_websocket_ports(
                &cfg.nacos,
                &bound_ports,
                &local_ip,
                &broker_id,
                &cfg.connect.websocket.path,
                &cfg.connect.protocol,
                cfg.nacos.region.as_deref(),
                cfg.nacos.priority,
                Some(&cfg.nacos.version),
            ).await {
                Ok(_) => {
                    info!("Nacos 服务注册完成");
                }
                Err(e) => {
                    tracing::warn!("Nacos 注册失败: {}", e);
                    tracing::warn!("应用将继续运行，但服务不会注册到 Nacos");
                    tracing::warn!("如果看到后续的 nacos_sdk 连接错误，请检查 Nacos 服务器配置");
                    tracing::warn!("常见问题：端口配置错误（应使用 8848 或 9848）、服务器不可访问、防火墙阻止");
                }
            }
        } else {
            info!("Nacos 注册未启用（使用 --nacos-register-enabled 启用）");
        }
    } else {
        info!("WebSocket 未启用，跳过 Nacos 注册");
    }

    // 连接 RabbitMQ
    let rabbitmq_host = cfg.rabbitmq.host.clone();
    let rabbitmq_port = cfg.rabbitmq.port;
    info!("正在连接 RabbitMQ: {}:{}", rabbitmq_host, rabbitmq_port);
    let amqp_url = format!(
        "amqp://{}:{}@{}:{}/{}",
        cfg.rabbitmq.username.as_deref().unwrap_or("guest"),
        cfg.rabbitmq.password.as_deref().unwrap_or("guest"),
        cfg.rabbitmq.host,
        cfg.rabbitmq.port,
        urlencoding::encode(&cfg.rabbitmq.virtual_host)
    );
    // 队列名优先级：命令行参数 rabbitmq_queue > broker_id
    let queue_name = cli.rabbitmq_queue.clone().unwrap_or_else(|| broker_id.clone());
    let exchange = cfg.rabbitmq.exchange.clone();
    let error_rk = cfg.rabbitmq.error_queue.clone();
    tokio::spawn(async move {
        if let Err(e) =
            mq::run_consumer(&amqp_url, &queue_name, &exchange, &error_rk, channel_map).await
        {
            tracing::error!("RabbitMQ 消费者退出 ({}:{}) - {}。请检查 RabbitMQ 服务是否已启动", 
                rabbitmq_host, rabbitmq_port, e);
        }
    });

    info!("im-connect 已启动");
    futures::future::pending::<()>().await;
    Ok(())
}
