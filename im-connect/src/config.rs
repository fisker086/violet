//! 配置：从命令行参数构建

use std::net::SocketAddr;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub connect: ConnectConfig,
    pub redis: RedisConfig,
    pub rabbitmq: RabbitMqConfig,
    pub nacos: NacosConfig,
    pub jwt: JwtConfig,
    /// 当前节点 ID，用作 RabbitMQ 队列名与 Nacos 实例标识
    /// 优先级：命令行参数 > 环境变量 BROKER_ID > 自动生成（主机名+随机后缀）
    pub broker_id: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub bind: String,
}

#[derive(Debug, Clone)]
pub struct ConnectConfig {
    /// 序列化协议：proto 或 json
    #[allow(dead_code)]
    pub protocol: String,
    /// 心跳超时时间（毫秒）
    pub heart_beat_time_ms: u64,
    /// 连接超时时间（毫秒）
    pub timeout_ms: u64,
    /// 是否允许多设备同时在线
    pub multi_device_enabled: bool,
    /// Boss 线程池大小（WebSocket 连接接受）
    #[allow(dead_code)]
    pub boss_thread_size: usize,
    /// Worker 线程池大小（WebSocket 消息处理）
    #[allow(dead_code)]
    pub work_thread_size: usize,
    pub websocket: WebSocketConfig,
}

#[derive(Debug, Clone)]
pub struct WebSocketConfig {
    pub enable: bool,
    pub path: String,
    pub ports: Vec<u16>,
}

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub database: u8,
    pub password: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RabbitMqConfig {
    pub host: String,
    pub port: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    pub virtual_host: String,
    pub exchange: String,
    pub error_queue: String,
}

#[derive(Debug, Clone)]
pub struct NacosConfig {
    pub server_addr: String,
    pub namespace: Option<String>,
    pub group: String,
    pub username: Option<String>,
    pub password: Option<String>,
    pub service_name: String,
    pub register_enabled: bool,
    pub region: Option<String>,
    pub priority: Option<i32>,
    pub version: String,
}

#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// JWT 签名密钥（生产环境必须修改）
    pub secret: String,
    /// Token 过期时间（小时）
    #[allow(dead_code)]
    pub expiration_hours: i64,
}

impl AppConfig {
    /// 从命令行参数构建配置（不使用配置文件）
    pub fn from_cli(cli: &crate::cli::Cli) -> Self {
        Self {
            server: ServerConfig {
                bind: cli.bind.clone(),
            },
            connect: ConnectConfig {
                protocol: "proto".to_string(),
                heart_beat_time_ms: cli.heart_beat_time_ms,
                timeout_ms: cli.timeout_ms,
                multi_device_enabled: cli.multi_device_enabled,
                boss_thread_size: 4,
                work_thread_size: 16,
                websocket: WebSocketConfig {
                    enable: cli.websocket_enable,
                    path: cli.websocket_path.clone(),
                    ports: cli.websocket_ports.clone().unwrap_or_else(|| vec![19000]),
                },
            },
            redis: RedisConfig {
                host: cli.redis_host.clone(),
                port: cli.redis_port,
                database: cli.redis_database,
                password: cli.redis_password.clone(),
            },
            rabbitmq: RabbitMqConfig {
                host: cli.rabbitmq_host.clone(),
                port: cli.rabbitmq_port,
                username: Some(cli.rabbitmq_username.clone()),
                password: Some(cli.rabbitmq_password.clone()),
                virtual_host: cli.rabbitmq_vhost.clone(),
                exchange: cli.rabbitmq_exchange.clone(),
                error_queue: cli.rabbitmq_error_queue.clone(),
            },
            nacos: NacosConfig {
                server_addr: cli.nacos_server_addr.clone(),
                namespace: cli.nacos_namespace.clone(),
                group: cli.nacos_group.clone(),
                username: cli.nacos_username.clone(),
                password: cli.nacos_password.clone(),
                service_name: cli.nacos_service_name.clone(),
                register_enabled: cli.nacos_register_enabled,
                region: cli.nacos_region.clone(),
                priority: cli.nacos_priority.or(Some(1)), // 默认优先级为 1
                version: cli.nacos_version.clone(),
            },
            jwt: JwtConfig {
                secret: cli.jwt_secret.clone(),
                expiration_hours: cli.jwt_expiration_hours,
            },
            broker_id: cli.broker_id.clone()
                .or_else(|| std::env::var("BROKER_ID").ok())
                .or_else(|| Some(generate_broker_id())),
        }
    }

    
    /// 获取 broker_id（保证非空）
    pub fn broker_id(&self) -> &str {
        self.broker_id.as_deref().unwrap_or("unknown")
    }
}

/// 自动生成 broker_id：主机名 + 随机后缀
fn generate_broker_id() -> String {
    let hostname = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| {
            // 尝试获取系统主机名
            hostname::get()
                .ok()
                .and_then(|h| h.to_str().map(|s| s.to_string()))
                .unwrap_or_else(|| "unknown".to_string())
        });
    
    // 清理主机名（移除特殊字符，只保留字母数字和连字符）
    let clean_hostname: String = hostname
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    
    // 使用主机名 + UUID 后缀，确保唯一性
    let suffix = uuid::Uuid::new_v4()
        .to_string()
        .chars()
        .take(8)
        .collect::<String>();
    
    format!("im-connect-{}-{}", clean_hostname, suffix)
}

/// 解析 WebSocket 监听地址
pub fn ws_bind_addr(bind: &str, port: u16) -> SocketAddr {
    format!("{}:{}", bind, port).parse().unwrap_or_else(|_| {
        format!("0.0.0.0:{}", port).parse().expect("fallback addr")
    })
}
