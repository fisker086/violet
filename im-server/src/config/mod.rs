use serde::Deserialize;
use std::{fs, path::Path};
use im_share::JwtSettings;

#[derive(Debug, Clone, Deserialize)]
pub struct MqttSettings {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerSettings {
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseSettings {
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub database: String,
}


#[derive(Debug, Clone, Deserialize)]
pub struct RedisSettings {
    pub host: String,
    #[serde(default = "default_redis_port")]
    pub port: u16,
    #[serde(default = "default_redis_db")]
    pub db: u8,
    #[serde(default)]
    pub password: Option<String>,
}

fn default_redis_port() -> u16 {
    6379
}

fn default_redis_db() -> u8 {
    0
}

#[derive(Debug, Clone, Deserialize)]
pub struct UploadSettings {
    #[serde(default = "default_upload_path")]
    pub path: String,
    #[serde(default = "default_max_image_size_mb")]
    pub max_image_size_mb: u64,
    #[serde(default = "default_max_file_size_mb")]
    pub max_file_size_mb: u64,
    #[serde(default = "default_enable_image_processing")]
    pub enable_image_processing: bool,
    #[serde(default = "default_image_quality")]
    #[allow(dead_code)]
    pub image_quality: u8,
    #[serde(default = "default_thumbnail_max_width")]
    pub thumbnail_max_width: u32,
    #[serde(default = "default_thumbnail_max_height")]
    pub thumbnail_max_height: u32,
    #[serde(default = "default_save_original")]
    pub save_original: bool,
}

fn default_upload_path() -> String {
    "uploads".to_string()
}

fn default_max_image_size_mb() -> u64 {
    10
}

fn default_max_file_size_mb() -> u64 {
    50
}

fn default_enable_image_processing() -> bool {
    true
}

fn default_image_quality() -> u8 {
    85
}

fn default_thumbnail_max_width() -> u32 {
    800
}

fn default_thumbnail_max_height() -> u32 {
    800
}

fn default_save_original() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
pub struct SrsSettings {
    #[serde(default = "default_srs_host")]
    pub host: String,  // 后端访问 SRS API 的地址（容器内使用 srs，本地使用 127.0.0.1）
    #[serde(default = "default_srs_http_host")]
    pub http_host: String,  // 后端访问 SRS HTTP 的地址
    #[serde(default = "default_srs_webrtc_port")]
    pub webrtc_port: u16,
    #[serde(default = "default_srs_app")]
    pub app: String,
    #[serde(default = "default_srs_client_host")]
    pub client_host: String,  // 前端访问 SRS 的地址（通常是 localhost 或公网 IP）
    #[serde(default = "default_srs_client_http_host")]
    pub client_http_host: String,  // 前端访问 SRS HTTP 的地址
}

fn default_srs_host() -> String {
    "http://127.0.0.1:1985".to_string()
}

fn default_srs_http_host() -> String {
    "http://127.0.0.1:8080".to_string()
}

fn default_srs_webrtc_port() -> u16 {
    8000
}

fn default_srs_app() -> String {
    "live".to_string()
}

fn default_srs_client_host() -> String {
    "http://127.0.0.1:1985".to_string()
}

fn default_srs_client_http_host() -> String {
    "http://127.0.0.1:8080".to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub mqtt: MqttSettings,
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub jwt: JwtSettings,
    #[serde(default = "default_redis_settings")]
    pub redis: RedisSettings,
    #[serde(default = "default_upload_settings")]
    pub upload: UploadSettings,
    #[serde(default = "default_srs_settings")]
    pub srs: SrsSettings,
}

fn default_srs_settings() -> SrsSettings {
    SrsSettings {
        host: "http://127.0.0.1:1985".to_string(),
        http_host: "http://127.0.0.1:8080".to_string(),
        webrtc_port: 8000,
        app: "live".to_string(),
        client_host: "http://127.0.0.1:1985".to_string(),
        client_http_host: "http://127.0.0.1:8080".to_string(),
    }
}

fn default_upload_settings() -> UploadSettings {
    UploadSettings {
        path: "uploads".to_string(),
        max_image_size_mb: 10,
        max_file_size_mb: 50,
        enable_image_processing: true,
        image_quality: 85,
        thumbnail_max_width: 800,
        thumbnail_max_height: 800,
        save_original: true,
    }
}

fn default_redis_settings() -> RedisSettings {
    RedisSettings {
        host: "127.0.0.1".to_string(),
        port: 6379,
        db: 0,
        password: None,
    }
}

impl AppConfig {
    pub fn load() -> Self {
        let path = std::env::var("IM_SERVER_CONFIG")
            .unwrap_or_else(|_| "im-server/config.toml".to_string());
        
        let mut config: AppConfig = fs::read_to_string(Path::new(&path)).map(|content| {
            toml::from_str(&content).expect("invalid im-server config")
        }).unwrap_or_else(|_| {
            let default_content = r#"
[mqtt]
host = "127.0.0.1"
port = 1883

[server]
port = 3000

[database]
host = "127.0.0.1"
port = 3306
user = "root"
password = "123456"
database = "violet"

[jwt]
secret = "your-secret-key-change-in-production"
expiration_hours = 24

[upload]
path = "uploads"
max_image_size_mb = 10
max_file_size_mb = 50
enable_image_processing = true
image_quality = 85
thumbnail_max_width = 800
thumbnail_max_height = 800
save_original = true

[srs]
host = "http://127.0.0.1:1985"
http_host = "http://127.0.0.1:8080"
webrtc_port = 8000
app = "live"
client_host = "http://127.0.0.1:1985"
client_http_host = "http://127.0.0.1:8080"
"#;
            toml::from_str(default_content).expect("invalid default config")
        });
        
        // 支持环境变量覆盖 SRS 配置
        if let Ok(srs_host) = std::env::var("SRS_HOST") {
            config.srs.host = srs_host;
        }
        if let Ok(srs_http_host) = std::env::var("SRS_HTTP_HOST") {
            config.srs.http_host = srs_http_host;
        }
        if let Ok(srs_webrtc_port) = std::env::var("SRS_WEBRTC_PORT") {
            if let Ok(port) = srs_webrtc_port.parse::<u16>() {
                config.srs.webrtc_port = port;
            }
        }
        if let Ok(srs_app) = std::env::var("SRS_APP") {
            config.srs.app = srs_app;
        }
        if let Ok(srs_client_host) = std::env::var("SRS_CLIENT_HOST") {
            config.srs.client_host = srs_client_host;
        }
        if let Ok(srs_client_http_host) = std::env::var("SRS_CLIENT_HTTP_HOST") {
            config.srs.client_http_host = srs_client_http_host;
        }
        
        config
    }
}

