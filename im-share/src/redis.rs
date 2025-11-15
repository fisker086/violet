use redis::Client;
use redis::aio::ConnectionManager;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

/// Redis 配置
#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub host: String,
    pub port: u16,
    pub db: u8,
    pub password: Option<String>,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port: 6379,
            db: 0,
            password: None,
        }
    }
}

impl RedisConfig {
    pub fn new(host: String, port: u16, db: u8, password: Option<String>) -> Self {
        Self {
            host,
            port,
            db,
            password,
        }
    }
}

/// Redis 客户端封装（共享模块，供 im-connect 和 im-server 使用）
pub struct RedisClient {
    manager: Arc<Mutex<ConnectionManager>>,
}

impl RedisClient {
    /// 创建 Redis 客户端
    pub async fn new(config: &RedisConfig) -> anyhow::Result<Self> {
        // 构建 Redis URL
        let url = if let Some(ref password) = config.password {
            format!("redis://:{}@{}:{}/{}", password, config.host, config.port, config.db)
        } else {
            format!("redis://{}:{}/{}", config.host, config.port, config.db)
        };
        
        info!("连接 Redis: {}:{}/{}", config.host, config.port, config.db);
        
        let client = Client::open(url)?;
        let manager = ConnectionManager::new(client).await?;
        
        // 测试连接
        let mut conn = manager.clone();
        redis::cmd("PING").query_async::<String>(&mut conn).await?;
        
        info!("Redis 连接成功");
        
        Ok(Self {
            manager: Arc::new(Mutex::new(manager)),
        })
    }
    
    /// 获取连接管理器（用于执行 Redis 命令）
    pub async fn get_connection(&self) -> ConnectionManager {
        self.manager.lock().await.clone()
    }
    
    /// 设置键值对（带过期时间，单位：秒）
    pub async fn set_with_ttl(&self, key: &str, value: &str, ttl: u64) -> Result<(), redis::RedisError> {
        let mut conn = self.get_connection().await;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .arg("EX")
            .arg(ttl)
            .query_async(&mut conn)
            .await
    }
    
    /// 设置键值对（永久）
    pub async fn set(&self, key: &str, value: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.get_connection().await;
        redis::cmd("SET")
            .arg(key)
            .arg(value)
            .query_async(&mut conn)
            .await
    }
    
    /// 获取值
    pub async fn get(&self, key: &str) -> Result<Option<String>, redis::RedisError> {
        let mut conn = self.get_connection().await;
        redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
    }
    
    /// 删除键
    pub async fn del(&self, key: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.get_connection().await;
        redis::cmd("DEL")
            .arg(key)
            .query_async(&mut conn)
            .await
    }
    
    /// 删除多个键
    pub async fn del_many(&self, keys: &[&str]) -> Result<(), redis::RedisError> {
        if keys.is_empty() {
            return Ok(());
        }
        let mut conn = self.get_connection().await;
        let mut cmd = redis::cmd("DEL");
        for key in keys {
            cmd.arg(key);
        }
        cmd.query_async(&mut conn).await
    }
    
    /// 检查键是否存在
    pub async fn exists(&self, key: &str) -> Result<bool, redis::RedisError> {
        let mut conn = self.get_connection().await;
        let result: i64 = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut conn)
            .await?;
        Ok(result > 0)
    }
    
    /// 设置过期时间
    pub async fn expire(&self, key: &str, seconds: u64) -> Result<(), redis::RedisError> {
        let mut conn = self.get_connection().await;
        redis::cmd("EXPIRE")
            .arg(key)
            .arg(seconds)
            .query_async(&mut conn)
            .await
    }
    
    
    /// 添加离线消息到队列（使用 open_id）
    /// 使用 Redis List 存储，key: offline:message:{open_id}
    /// 使用 RPUSH 将新消息追加到列表末尾，确保消息按时间顺序（从旧到新）存储
    pub async fn add_offline_message(&self, open_id: &str, message: &str) -> Result<(), redis::RedisError> {
        use tracing::info;
        let key = format!("offline:message:{}", open_id);
        let mut conn = self.get_connection().await;
        
        // 尝试解析消息以获取 chat_type 用于日志
        let chat_type_info = if let Ok(json) = serde_json::from_str::<serde_json::Value>(message) {
            format!("chat_type={:?}", json.get("chat_type"))
        } else {
            "无法解析JSON".to_string()
        };
        
        info!(
            open_id = %open_id, 
            key = %key, 
            %chat_type_info,
            message_preview = if message.len() > 100 { format!("{}...", &message[..100]) } else { message.to_string() },
            "执行Redis RPUSH操作，存储离线消息"
        );
        
        let result = redis::cmd("RPUSH")
            .arg(&key)
            .arg(message)
            .query_async::<i64>(&mut conn)
            .await?;
        
        info!(
            open_id = %open_id, 
            key = %key, 
            list_length = result, 
            %chat_type_info,
            "✅ Redis RPUSH成功，列表长度: {}",
            result
        );
        
        // 设置过期时间：7天
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(604800u64) // 7 * 24 * 60 * 60
            .query_async::<()>(&mut conn)
            .await?;
        
        info!(
            open_id = %open_id, 
            key = %key, 
            %chat_type_info,
            "Redis EXPIRE设置成功，过期时间7天"
        );
        
        Ok(())
    }
    
    /// 获取并删除所有离线消息（使用 open_id）
    /// 返回消息列表，按时间顺序（从旧到新）
    /// LRANGE 0 -1 从左到右获取所有消息，由于使用 RPUSH，消息已按从旧到新顺序存储
    pub async fn get_and_clear_offline_messages(&self, open_id: &str) -> Result<Vec<String>, redis::RedisError> {
        use tracing::{info, debug};
        let key = format!("offline:message:{}", open_id);
        let mut conn = self.get_connection().await;
        
        // 获取所有消息（从左到右，即从旧到新，因为使用 RPUSH 追加）
        // 如果 key 不存在，LRANGE 会返回空列表，无需先检查 EXISTS
        let messages: Vec<String> = redis::cmd("LRANGE")
            .arg(&key)
            .arg(0)
            .arg(-1)
            .query_async(&mut conn)
            .await?;
        
        // 只在有消息时输出详细日志，没有消息时使用 debug 级别
        if !messages.is_empty() {
            info!(
                open_id = %open_id, 
                key = %key, 
                message_count = messages.len(), 
                "从Redis获取到 {} 条离线消息", 
                messages.len()
            );
            
            // 删除key
            redis::cmd("DEL")
                .arg(&key)
                .query_async::<()>(&mut conn)
                .await?;
            debug!(open_id = %open_id, key = %key, "已删除Redis离线消息key");
        } else {
            debug!(
                open_id = %open_id, 
                key = %key, 
                "Redis中没有离线消息"
            );
        }
        
        Ok(messages)
    }
    
    /// 获取离线消息数量（使用 open_id）
    pub async fn get_offline_message_count(&self, open_id: &str) -> Result<usize, redis::RedisError> {
        let key = format!("offline:message:{}", open_id);
        let mut conn = self.get_connection().await;
        let count: i64 = redis::cmd("LLEN")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        Ok(count as usize)
    }

    // ========== 群消息已读状态相关方法（使用 Redis Set） ==========
    
    /// 标记群消息为已读
    /// key: group:read:{group_id}:{message_id}
    /// 使用 Set 存储已读用户的 open_id
    pub async fn mark_group_message_read(&self, group_id: &str, message_id: &str, user_id: &str) -> Result<(), redis::RedisError> {
        let key = format!("group:read:{}:{}", group_id, message_id);
        let mut conn = self.get_connection().await;
        let _: i64 = redis::cmd("SADD")
            .arg(&key)
            .arg(user_id)
            .query_async(&mut conn)
            .await?;
        // 设置过期时间：30天
        let _: i64 = redis::cmd("EXPIRE")
            .arg(&key)
            .arg(2592000u64) // 30 * 24 * 60 * 60
            .query_async(&mut conn)
            .await?;
        Ok(())
    }
    
    /// 检查用户是否已读群消息
    pub async fn is_group_message_read(&self, group_id: &str, message_id: &str, user_id: &str) -> Result<bool, redis::RedisError> {
        let key = format!("group:read:{}:{}", group_id, message_id);
        let mut conn = self.get_connection().await;
        let result: i64 = redis::cmd("SISMEMBER")
            .arg(&key)
            .arg(user_id)
            .query_async(&mut conn)
            .await?;
        Ok(result > 0)
    }
    
    /// 获取群消息的已读用户列表
    pub async fn get_group_message_read_users(&self, group_id: &str, message_id: &str) -> Result<Vec<String>, redis::RedisError> {
        let key = format!("group:read:{}:{}", group_id, message_id);
        let mut conn = self.get_connection().await;
        let users: Vec<String> = redis::cmd("SMEMBERS")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        Ok(users)
    }
    
    /// 获取群消息的已读数量
    pub async fn get_group_message_read_count(&self, group_id: &str, message_id: &str) -> Result<usize, redis::RedisError> {
        let key = format!("group:read:{}:{}", group_id, message_id);
        let mut conn = self.get_connection().await;
        let count: i64 = redis::cmd("SCARD")
            .arg(&key)
            .query_async(&mut conn)
            .await?;
        Ok(count as usize)
    }
    
    /// 批量标记群消息为已读
    pub async fn mark_group_messages_read(&self, group_id: &str, message_ids: &[&str], user_id: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.get_connection().await;
        for message_id in message_ids {
            let key = format!("group:read:{}:{}", group_id, message_id);
            let _: i64 = redis::cmd("SADD")
                .arg(&key)
                .arg(user_id)
                .query_async(&mut conn)
                .await?;
            // 设置过期时间：30天
            let _: i64 = redis::cmd("EXPIRE")
                .arg(&key)
                .arg(2592000u64)
                .query_async(&mut conn)
                .await?;
        }
        Ok(())
    }
}

