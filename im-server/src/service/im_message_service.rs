use crate::model::{ImSingleMessage, ImGroupMessage, ImGroupMessageStatus};
use crate::error::{ErrorCode, Result};
use sqlx::MySqlPool;
use im_share::{now_timestamp, RedisClient};
use std::sync::Arc;

pub struct ImMessageService {
    pool: MySqlPool,
    redis: Option<Arc<RedisClient>>,
}

impl ImMessageService {
    pub fn new(pool: MySqlPool) -> Self {
        Self { 
            pool,
            redis: None,
        }
    }
    
    pub fn with_redis(pool: MySqlPool, redis: Arc<RedisClient>) -> Self {
        Self {
            pool,
            redis: Some(redis),
        }
    }


    /// 保存单聊消息
    pub async fn save_single_message(&self, message: ImSingleMessage) -> Result<()> {
        let now = now_timestamp();
        use tracing::error;

        let result = sqlx::query(
            "INSERT INTO im_single_message 
             (message_id, from_id, to_id, message_body, message_time, message_content_type, 
              read_status, extra, del_flag, sequence, message_random, create_time, update_time, version, reply_to,
              to_type, file_url, file_name, file_type) 
             VALUES (?, ?, ?, ?, ?, ?, 0, ?, 1, ?, ?, ?, ?, 1, ?, ?, ?, ?, ?)
             ON DUPLICATE KEY UPDATE message_id = message_id"
        )
        .bind(&message.message_id)
        .bind(&message.from_id)
        .bind(&message.to_id)
        .bind(&message.message_body)
        .bind(message.message_time)
        .bind(message.message_content_type)
        .bind(&message.extra)
        .bind(message.sequence)
        .bind(&message.message_random)
        .bind(now)
        .bind(now)
        .bind(&message.reply_to)
        .bind(&message.to_type)
        .bind(&message.file_url)
        .bind(&message.file_name)
        .bind(&message.file_type)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("保存单聊消息到数据库失败: {:?}, 消息: from_id={}, to_id={}, message_id={}", 
                    e, message.from_id, message.to_id, message.message_id);
                // 输出更详细的错误信息
                if let sqlx::Error::Database(db_err) = &e {
                    error!("数据库错误详情: {:?}, SQL错误: {}", db_err, db_err.message());
                }
                Err(ErrorCode::Database)
            }
        }
    }

    /// 获取单聊消息列表
    /// 重要：过滤掉通话邀请消息（message_content_type = 4），因为通话邀请是实时消息，过期后没有意义
    pub async fn get_single_messages(&self, from_id: &str, to_id: &str, since_sequence: Option<i64>, limit: i32) -> Result<Vec<ImSingleMessage>> {
        let mut query = "SELECT message_id, from_id, to_id, message_body, message_time, message_content_type, 
                                read_status, extra, del_flag, sequence, message_random, create_time, update_time, version, reply_to,
                                to_type, file_url, file_name, file_type
                         FROM im_single_message 
                         WHERE ((from_id = ? AND to_id = ?) OR (from_id = ? AND to_id = ?)) 
                         AND del_flag = 1 AND message_content_type != 4".to_string();

        if let Some(seq) = since_sequence {
            query.push_str(&format!(" AND sequence > {}", seq));
        }

        query.push_str(" ORDER BY sequence ASC LIMIT ?");

        let messages = sqlx::query_as::<_, ImSingleMessage>(&query)
            .bind(from_id)
            .bind(to_id)
            .bind(to_id)
            .bind(from_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        Ok(messages)
    }

    /// 标记消息为已读
    pub async fn mark_single_message_read(&self, message_id: &str, to_id: &str) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "UPDATE im_single_message 
             SET read_status = 1, update_time = ?, version = version + 1 
             WHERE message_id = ? AND to_id = ?"
        )
        .bind(now)
        .bind(message_id)
        .bind(to_id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 保存群聊消息
    pub async fn save_group_message(&self, message: ImGroupMessage) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "INSERT INTO im_group_message 
             (message_id, group_id, from_id, message_body, message_time, message_content_type, 
              extra, del_flag, sequence, message_random, create_time, update_time, version, reply_to) 
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?, ?, 1, ?)
             ON DUPLICATE KEY UPDATE message_id = message_id"
        )
        .bind(&message.message_id)
        .bind(&message.group_id)
        .bind(&message.from_id)
        .bind(&message.message_body)
        .bind(message.message_time)
        .bind(message.message_content_type)
        .bind(&message.extra)
        .bind(message.sequence)
        .bind(&message.message_random)
        .bind(now)
        .bind(now)
        .bind(&message.reply_to)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 获取群聊消息列表
    /// 重要：过滤掉通话邀请消息（message_content_type = 4），因为通话邀请是实时消息，过期后没有意义
    pub async fn get_group_messages(&self, group_id: &str, since_sequence: Option<i64>, limit: i32) -> Result<Vec<ImGroupMessage>> {
        let mut query = "SELECT message_id, group_id, from_id, message_body, message_time, message_content_type, 
                                extra, del_flag, sequence, message_random, create_time, update_time, version, reply_to 
                         FROM im_group_message 
                         WHERE group_id = ? AND del_flag = 1 AND message_content_type != 4".to_string();

        if let Some(seq) = since_sequence {
            query.push_str(&format!(" AND sequence > {}", seq));
        }

        query.push_str(" ORDER BY sequence ASC LIMIT ?");

        let messages = sqlx::query_as::<_, ImGroupMessage>(&query)
            .bind(group_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        Ok(messages)
    }

    /// 标记群消息为已读（使用 Redis）
    pub async fn mark_group_message_read(&self, group_id: &str, message_id: &str, to_id: &str) -> Result<()> {
        if let Some(ref redis) = self.redis {
            redis.mark_group_message_read(group_id, message_id, to_id)
                .await
                .map_err(|_| ErrorCode::Internal)?;
            Ok(())
        } else {
            Err(ErrorCode::Internal)
        }
    }

    /// 获取群消息的已读状态（使用 Redis）
    pub async fn get_group_message_status(&self, group_id: &str, message_id: &str) -> Result<Vec<ImGroupMessageStatus>> {
        if let Some(ref redis) = self.redis {
            let user_ids = redis.get_group_message_read_users(group_id, message_id)
                .await
                .map_err(|_| ErrorCode::Internal)?;
            
            // 转换为 ImGroupMessageStatus 格式
            let statuses = user_ids.into_iter().map(|to_id| {
                ImGroupMessageStatus {
                    group_id: group_id.to_string(),
                    message_id: message_id.to_string(),
                    to_id,
                    read_status: Some(1),
                    create_time: Some(now_timestamp()),
                    update_time: Some(now_timestamp()),
                    version: Some(1),
                }
            }).collect();
            
            Ok(statuses)
        } else {
            Err(ErrorCode::Internal)
        }
    }

    /// 获取用户在群组中的消息已读状态（使用 Redis）
    /// 注意：这个方法在 Redis 模式下需要遍历所有消息，性能可能不如数据库
    /// 建议使用 get_group_message_read_count 来获取已读数量
    pub async fn get_user_group_message_status(&self, _group_id: &str, _to_id: &str, _limit: Option<i32>) -> Result<Vec<ImGroupMessageStatus>> {
        // Redis 模式下，这个方法不太实用，因为需要遍历所有消息
        // 暂时返回空列表，或者可以从数据库获取消息列表，然后检查 Redis
        if self.redis.is_some() {
            Ok(vec![])
        } else {
            Err(ErrorCode::Internal)
        }
    }
    
    /// 检查用户是否已读群消息（使用 Redis）
    #[allow(dead_code)]
    pub async fn is_group_message_read(&self, group_id: &str, message_id: &str, to_id: &str) -> Result<bool> {
        if let Some(ref redis) = self.redis {
            let result = redis.is_group_message_read(group_id, message_id, to_id)
                .await
                .map_err(|_| ErrorCode::Internal)?;
            Ok(result)
        } else {
            Err(ErrorCode::Internal)
        }
    }
    
    /// 获取群消息的已读数量（使用 Redis）
    #[allow(dead_code)]
    pub async fn get_group_message_read_count(&self, group_id: &str, message_id: &str) -> Result<usize> {
        if let Some(ref redis) = self.redis {
            let count = redis.get_group_message_read_count(group_id, message_id)
                .await
                .map_err(|_| ErrorCode::Internal)?;
            Ok(count)
        } else {
            Err(ErrorCode::Internal)
        }
    }
}

