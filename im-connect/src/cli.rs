//! 命令行参数定义（使用 clap）

use clap::Parser;

/// im-connect WebSocket 长连接网关
#[derive(Parser, Debug, Clone)]
#[command(name = "im-connect")]
#[command(version)]
#[command(about = "WebSocket 长连接网关，支持 JWT 鉴权、Redis 会话、RabbitMQ 消息队列、Nacos 服务发现")]
#[command(long_about = None)]
pub struct Cli {

    /// 日志级别（trace, debug, info, warn, error）
    #[arg(short, long, default_value = "info", value_name = "LEVEL", help = "日志级别: trace, debug, info, warn, error")]
    pub log_level: String,

    // ========== Broker ID ==========
    /// Broker ID（用作 RabbitMQ 队列名）
    #[arg(long, value_name = "BROKER_ID", help = "Broker ID，用作 RabbitMQ 队列名。优先级：命令行 > 环境变量 BROKER_ID > 配置文件 > 自动生成")]
    pub broker_id: Option<String>,

    // ========== Server ==========
    /// 服务器绑定地址
    #[arg(long, default_value = "0.0.0.0", help = "服务器绑定地址，默认: 0.0.0.0")]
    pub bind: String,

    // ========== WebSocket ==========
    /// 是否启用 WebSocket
    #[arg(long, default_value = "true", help = "是否启用 WebSocket，默认: true")]
    pub websocket_enable: bool,

    /// WebSocket 路径
    #[arg(long, default_value = "/im", help = "WebSocket 路径，默认: /im")]
    pub websocket_path: String,

    /// WebSocket 监听端口（可指定多个，用逗号分隔）
    #[arg(long, value_delimiter = ',', help = "WebSocket 监听端口，可指定多个（用逗号分隔），默认: 19000。示例: --websocket-ports 19000,19001,19002")]
    pub websocket_ports: Option<Vec<u16>>,

    // ========== Connect ==========
    /// 心跳超时时间（毫秒）
    #[arg(long, default_value = "30000", help = "心跳超时时间（毫秒），默认: 30000")]
    pub heart_beat_time_ms: u64,

    /// 连接超时时间（毫秒）
    #[arg(long, default_value = "60000", help = "连接超时时间（毫秒），默认: 60000")]
    pub timeout_ms: u64,

    /// 是否允许多设备同时在线
    #[arg(long, default_value = "true", help = "是否允许多设备同时在线，默认: true")]
    pub multi_device_enabled: bool,

    // ========== Redis ==========
    /// Redis 主机地址
    #[arg(long, default_value = "127.0.0.1", help = "Redis 主机地址，默认: 127.0.0.1")]
    pub redis_host: String,

    /// Redis 端口
    #[arg(long, default_value = "6379", help = "Redis 端口，默认: 6379")]
    pub redis_port: u16,

    /// Redis 数据库编号
    #[arg(long, default_value = "0", help = "Redis 数据库编号，默认: 0")]
    pub redis_database: u8,

    /// Redis 密码（可选）
    #[arg(long, help = "Redis 密码（可选）")]
    pub redis_password: Option<String>,

    // ========== RabbitMQ ==========
    /// RabbitMQ 主机地址
    #[arg(long, default_value = "127.0.0.1", help = "RabbitMQ 主机地址，默认: 127.0.0.1")]
    pub rabbitmq_host: String,

    /// RabbitMQ 端口
    #[arg(long, default_value = "5672", help = "RabbitMQ 端口，默认: 5672")]
    pub rabbitmq_port: u16,

    /// RabbitMQ 用户名
    #[arg(long, default_value = "guest", help = "RabbitMQ 用户名，默认: guest")]
    pub rabbitmq_username: String,

    /// RabbitMQ 密码
    #[arg(long, default_value = "guest", help = "RabbitMQ 密码，默认: guest")]
    pub rabbitmq_password: String,

    /// RabbitMQ 虚拟主机
    #[arg(long, default_value = "/", help = "RabbitMQ 虚拟主机，默认: /")]
    pub rabbitmq_vhost: String,

    /// RabbitMQ Exchange 名称
    #[arg(long, default_value = "IM-SERVER", help = "RabbitMQ Exchange 名称，默认: IM-SERVER")]
    pub rabbitmq_exchange: String,

    /// RabbitMQ 队列名称（如果不指定，将使用 broker-id 的值）
    #[arg(long, help = "RabbitMQ 队列名称。如果不指定，将使用 --broker-id 的值作为队列名")]
    pub rabbitmq_queue: Option<String>,

    /// RabbitMQ 错误队列名称
    #[arg(long, default_value = "im.error", help = "RabbitMQ 错误队列名称，默认: im.error")]
    pub rabbitmq_error_queue: String,

    // ========== Nacos ==========
    /// Nacos 服务器地址
    #[arg(long, default_value = "127.0.0.1:8848", help = "Nacos 服务器地址，默认: 127.0.0.1:8848")]
    pub nacos_server_addr: String,

    /// Nacos 命名空间（可选）
    #[arg(long, help = "Nacos 命名空间（可选）")]
    pub nacos_namespace: Option<String>,

    /// Nacos 分组
    #[arg(long, default_value = "DEFAULT_GROUP", help = "Nacos 分组，默认: DEFAULT_GROUP")]
    pub nacos_group: String,

    /// Nacos 服务名（注册到 Nacos 的服务名称）
    #[arg(long, default_value = "im-connect", help = "Nacos 服务名（注册到 Nacos 的服务名称），默认: im-connect")]
    pub nacos_service_name: String,

    /// Nacos 用户名（可选，如果 Nacos 启用了认证）
    #[arg(long, help = "Nacos 用户名（可选），如果 Nacos 启用了认证")]
    pub nacos_username: Option<String>,

    /// Nacos 密码（可选，如果 Nacos 启用了认证）
    #[arg(long, help = "Nacos 密码（可选），如果 Nacos 启用了认证")]
    pub nacos_password: Option<String>,

    /// 是否启用 Nacos 注册
    #[arg(long, default_value = "false", help = "是否启用 Nacos 注册，默认: false")]
    pub nacos_register_enabled: bool,

    /// Nacos 区域（可选，用于服务实例 metadata）
    #[arg(long, help = "Nacos 区域（可选），例如: cn-shanghai")]
    pub nacos_region: Option<String>,

    /// Nacos 优先级（可选，用于服务实例 metadata，默认: 1）
    #[arg(long, help = "Nacos 优先级（可选），默认: 1")]
    pub nacos_priority: Option<i32>,

    /// Nacos 版本号（可选，用于服务实例 metadata，默认: 1.0.0）
    #[arg(long, default_value = "1.0.0", help = "Nacos 版本号（可选），默认: 1.0.0")]
    pub nacos_version: String,

    // ========== JWT ==========
    /// JWT 签名密钥
    #[arg(long, default_value = "iK4DTWZHnwpJb1GthwXuPSiwJZ4728gwX49l2y4Kf28=", help = "JWT 签名密钥（生产环境必须修改）")]
    pub jwt_secret: String,

    /// JWT Token 过期时间（小时）
    #[arg(long, default_value = "24", help = "JWT Token 过期时间（小时），默认: 24")]
    pub jwt_expiration_hours: i64,
}

impl Cli {
    /// 解析命令行参数
    pub fn parse() -> Self {
        Parser::parse()
    }

    /// 获取日志过滤器字符串
    pub fn log_filter(&self) -> String {
        std::env::var("RUST_LOG").unwrap_or_else(|_| self.log_level.clone())
    }
}
