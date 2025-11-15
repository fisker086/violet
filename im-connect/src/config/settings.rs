use serde::Deserialize;
use im_share::JwtSettings;

#[derive(Debug, Clone, Deserialize)]
pub struct MqttSettings {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ConnectSettings {
    pub port: u16,
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
pub struct AppConfig {
    pub mqtt: MqttSettings,
    pub connect: ConnectSettings,
    #[serde(default = "default_redis_settings")]
    pub redis: RedisSettings,
    #[serde(default = "default_jwt_settings")]
    pub jwt: JwtSettings,
}

fn default_redis_settings() -> RedisSettings {
    RedisSettings {
        host: "127.0.0.1".to_string(),
        port: 6379,
        db: 0,
        password: None,
    }
}

fn default_jwt_settings() -> JwtSettings {
    JwtSettings {
        secret: "your-secret-key-change-in-production".to_string(),
        expiration_hours: 24,
    }
}

