mod settings;

use std::{fs, path::Path};
pub use settings::AppConfig;

impl AppConfig {
    pub fn load() -> Self {
        // 优先从环境变量读取配置文件路径，确保与 im-server 完全独立
        let path = std::env::var("IM_CONNECT_CONFIG")
            .unwrap_or_else(|_| "im-connect/config.toml".to_string());
        
        // 读取配置文件，如果文件不存在则使用默认配置
        let config: AppConfig = fs::read_to_string(Path::new(&path)).map(|content| {
            toml::from_str(&content).expect(&format!("invalid im-connect config file: {}", path))
        }).unwrap_or_else(|_| {
            let default_content = r#"
[mqtt]
host = "127.0.0.1"
port = 1883

[connect]
port = 3001

[redis]
host = "127.0.0.1"
port = 6379
db = 0
# password = ""  # 如果需要密码，取消注释并填写

[jwt]
secret = "your-secret-key-change-in-production"
expiration_hours = 24
"#;
            toml::from_str(default_content).expect("invalid default config")
        });
        
        config
    }
}

