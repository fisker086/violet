pub mod mqtt;
pub mod model;
pub mod utils;
pub mod group;
pub mod subscription;
pub mod snowflake;
pub mod user;
pub mod redis;
pub mod auth;

// Re-exports for convenience
pub use mqtt::{ImMqtt, MqttConfig, IncomingMessage};
pub use model::{ChatMessage, Target, SendRequest};
pub use utils::{mqtt_user_topic, encode_message, decode_message, now_timestamp, now_timestamp_seconds};
pub use group::{get_group_members, set_group_members};
pub use subscription::{SubscriptionService, get_user_id_by_subscription, get_user_info_by_subscription};
pub use user::{get_snowflake_id_by_identifier, get_open_id_by_identifier};
pub use snowflake::{generate_snowflake_id, generate_snowflake_id_with_config};
pub use user::{get_username_by_id, clear_username_cache, get_cache_size};
pub use redis::{RedisClient, RedisConfig};
pub use auth::{JwtSettings, Claims, generate_token_with_open_id, generate_token, verify_token};

