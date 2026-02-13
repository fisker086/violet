use crate::config::RedisConfig;
use redis::Client;

/// Redis 异步连接类型别名（避免直接引用 redis::aio）
pub type RedisConnection = redis::aio::MultiplexedConnection;

/// Create a Redis client from configuration
pub fn get_redis_client(cfg: &RedisConfig) -> Result<Client, redis::RedisError> {
    let url = if let Some(password) = &cfg.password {
        format!("redis://:{}@{}:{}/{}", password, cfg.host, cfg.port, cfg.database)
    } else {
        format!("redis://{}:{}/{}", cfg.host, cfg.port, cfg.database)
    };
    
    Client::open(url)
}
