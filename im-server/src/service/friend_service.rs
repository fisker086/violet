use crate::model::User;
use crate::error::{ErrorCode, Result};
use sqlx::MySqlPool;
use im_share::now_timestamp;

pub struct FriendService {
    pool: MySqlPool,
}

impl FriendService {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }

    /// 添加好友请求（使用 im_friendship 表）
    pub async fn add_friend(&self, user_id: u64, friend_id: u64) -> Result<()> {
        if user_id == friend_id {
            return Err(ErrorCode::InvalidInput);
        }

        // 获取用户的 open_id
        let user1 = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let user2 = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(friend_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let owner_id = user1.ok_or(ErrorCode::NotFound)?;
        let to_id = user2.ok_or(ErrorCode::NotFound)?;

        // 检查是否已经是好友
        if self.is_friend(user_id, friend_id).await? {
            return Err(ErrorCode::InvalidInput); // 已经是好友
        }

        // 检查是否已经存在关系
        let existing_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) 
             FROM im_friendship 
             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
             AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(&owner_id)
        .bind(&to_id)
        .bind(&to_id)
        .bind(&owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        if existing_count > 0 {
            return Err(ErrorCode::InvalidInput); // 已经存在好友关系
        }

        // 插入好友关系（双向关系，自动接受）
        let now = now_timestamp();
        sqlx::query(
            "INSERT INTO im_friendship (owner_id, to_id, remark, del_flag, black, create_time, update_time, sequence, add_source, version) 
             VALUES (?, ?, NULL, 1, 1, ?, ?, ?, 'api', 1)"
        )
        .bind(&owner_id)
        .bind(&to_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        // 插入反向关系
        sqlx::query(
            "INSERT INTO im_friendship (owner_id, to_id, remark, del_flag, black, create_time, update_time, sequence, add_source, version) 
             VALUES (?, ?, NULL, 1, 1, ?, ?, ?, 'api', 1)"
        )
        .bind(&to_id)
        .bind(&owner_id)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 检查是否是好友（使用 im_friendship 表）
    pub async fn is_friend(&self, user_id: u64, friend_id: u64) -> Result<bool> {
        // 获取用户的 open_id
        let user1 = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let user2 = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(friend_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let owner_id = user1.ok_or(ErrorCode::NotFound)?;
        let to_id = user2.ok_or(ErrorCode::NotFound)?;

        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM im_friendship 
             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
             AND (del_flag IS NULL OR del_flag = 1)
             AND (black IS NULL OR black = 1)"
        )
        .bind(&owner_id)
        .bind(&to_id)
        .bind(&to_id)
        .bind(&owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(result > 0)
    }

    /// 获取好友列表（使用 im_friendship 表）
    pub async fn get_friends(&self, user_id: u64) -> Result<Vec<User>> {
        // 获取用户的 open_id
        let owner_id = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let owner_id = owner_id.ok_or(ErrorCode::NotFound)?;

        let friends = sqlx::query_as::<_, User>(
            "SELECT u.id, u.open_id, u.name, u.email, u.password_hash, u.file_name, u.abstract as abstract_field, u.phone, u.status, u.gender
             FROM im_friendship f 
             INNER JOIN users u ON f.to_id = u.open_id 
             WHERE f.owner_id = ? 
             AND (f.del_flag IS NULL OR f.del_flag = 1)
             AND (f.black IS NULL OR f.black = 1)
             AND (u.status IS NULL OR u.status = 1)
             ORDER BY f.update_time DESC"
        )
        .bind(&owner_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(friends)
    }

    /// 删除好友（使用 im_friendship 表）
    pub async fn remove_friend(&self, user_id: u64, friend_id: u64) -> Result<()> {
        // 获取用户的 open_id
        let user1 = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let user2 = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE id = ?"
        )
        .bind(friend_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let owner_id = user1.ok_or(ErrorCode::NotFound)?;
        let to_id = user2.ok_or(ErrorCode::NotFound)?;

        // 软删除双向关系（设置 del_flag = 0）
        let now = now_timestamp();
        sqlx::query(
            "UPDATE im_friendship 
             SET del_flag = 0, update_time = ?, version = version + 1
             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
             AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(now)
        .bind(&owner_id)
        .bind(&to_id)
        .bind(&to_id)
        .bind(&owner_id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }
}

