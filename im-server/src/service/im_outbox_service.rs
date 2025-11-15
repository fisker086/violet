use crate::model::ImOutbox;
use crate::error::{ErrorCode, Result};
use sqlx::MySqlPool;
use im_share::now_timestamp;

pub struct ImOutboxService {
    pool: MySqlPool,
}

impl ImOutboxService {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    /// 创建发件箱记录
    pub async fn create(&self, message_id: &str, payload: &str, exchange: &str, routing_key: &str) -> Result<ImOutbox> {
        let now = now_timestamp();

        let result = sqlx::query(
            "INSERT INTO im_outbox 
             (message_id, payload, exchange, routing_key, attempts, status, created_at, updated_at) 
             VALUES (?, ?, ?, ?, 0, 'PENDING', ?, ?)"
        )
        .bind(message_id)
        .bind(payload)
        .bind(exchange)
        .bind(routing_key)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let id = result.last_insert_id();
        self.get_by_id(id).await
    }

    /// 根据ID获取发件箱记录
    pub async fn get_by_id(&self, id: u64) -> Result<ImOutbox> {
        let outbox = sqlx::query_as::<_, ImOutbox>(
            "SELECT id, message_id, payload, exchange, routing_key, attempts, status, 
                    last_error, created_at, updated_at, next_try_at 
             FROM im_outbox 
             WHERE id = ?"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        match outbox {
            Some(o) => Ok(o),
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 根据消息ID获取发件箱记录
    #[allow(dead_code)]
    pub async fn get_by_message_id(&self, message_id: &str) -> Result<Option<ImOutbox>> {
        let outbox = sqlx::query_as::<_, ImOutbox>(
            "SELECT id, message_id, payload, exchange, routing_key, attempts, status, 
                    last_error, created_at, updated_at, next_try_at 
             FROM im_outbox 
             WHERE message_id = ?"
        )
        .bind(message_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(outbox)
    }

    /// 更新状态
    pub async fn update_status(&self, id: u64, status: &str) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "UPDATE im_outbox 
             SET status = ?, updated_at = ? 
             WHERE id = ?"
        )
        .bind(status)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 增加尝试次数并记录错误
    #[allow(dead_code)]
    pub async fn increment_attempts(&self, id: u64, error: Option<&str>) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "UPDATE im_outbox 
             SET attempts = attempts + 1, 
                 last_error = ?,
                 updated_at = ? 
             WHERE id = ?"
        )
        .bind(error)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 设置下次重试时间
    #[allow(dead_code)]
    pub async fn set_next_try_at(&self, id: u64, next_try_at: i64) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "UPDATE im_outbox 
             SET next_try_at = ?, updated_at = ? 
             WHERE id = ?"
        )
        .bind(next_try_at)
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 标记为已发送
    pub async fn mark_sent(&self, id: u64) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "UPDATE im_outbox 
             SET status = 'SENT', updated_at = ? 
             WHERE id = ?"
        )
        .bind(now)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 获取待发送的消息
    pub async fn get_pending_messages(&self, limit: i32) -> Result<Vec<ImOutbox>> {
        let now = now_timestamp();

        let messages = sqlx::query_as::<_, ImOutbox>(
            "SELECT id, message_id, payload, exchange, routing_key, attempts, status, 
                    last_error, created_at, updated_at, next_try_at 
             FROM im_outbox 
             WHERE status = 'PENDING' 
             AND (next_try_at IS NULL OR next_try_at <= ?)
             ORDER BY created_at ASC 
             LIMIT ?"
        )
        .bind(now)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(messages)
    }

    /// 获取失败的消息
    pub async fn get_failed_messages(&self, limit: i32) -> Result<Vec<ImOutbox>> {
        let messages = sqlx::query_as::<_, ImOutbox>(
            "SELECT id, message_id, payload, exchange, routing_key, attempts, status, 
                    last_error, created_at, updated_at, next_try_at 
             FROM im_outbox 
             WHERE status = 'FAILED' 
             ORDER BY updated_at DESC 
             LIMIT ?"
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(messages)
    }
}

