// src/service/user_service.rs
use crate::model::User;
use crate::error::{ErrorCode, Result};
use im_share::RedisClient;
use sqlx::MySqlPool;
use bcrypt::{hash, verify, DEFAULT_COST};
use im_share::generate_snowflake_id;
use std::sync::Arc;
use serde_json;

pub struct UserService {
    pool: MySqlPool,
    redis: Option<Arc<RedisClient>>,
}

impl UserService {
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
    
    // Redis 缓存键生成函数
    fn cache_key_user_id(&self, id: u64) -> String {
        format!("user:id:{}", id)
    }
    
    fn cache_key_open_id(&self, open_id: &str) -> String {
        format!("user:open_id:{}", open_id)
    }
    
    fn cache_key_name(&self, name: &str) -> String {
        format!("user:name:{}", name)
    }
    
    fn cache_key_email(&self, email: &str) -> String {
        format!("user:email:{}", email)
    }
    
    fn cache_key_phone(&self, phone: &str) -> String {
        format!("user:phone:{}", phone)
    }
    
    fn cache_key_online_status(&self, open_id: &str) -> String {
        format!("user:online:{}", open_id)
    }
    
    // 缓存用户信息（TTL: 1小时）
    async fn cache_user(&self, user: &User) -> Result<()> {
        if let Some(ref redis) = self.redis {
            let user_json = serde_json::to_string(user)
                .map_err(|_| ErrorCode::Internal)?;
            
            // 缓存用户信息，使用多个键
            let ttl = 3600u64; // 1小时
            
            // 通过 ID 缓存
            if let Err(e) = redis.set_with_ttl(&self.cache_key_user_id(user.id), &user_json, ttl).await {
                tracing::warn!(error = ?e, "缓存用户信息失败 (id)");
            }
            
            // 通过 open_id 缓存
            if let Some(ref open_id) = user.open_id {
                if let Err(e) = redis.set_with_ttl(&self.cache_key_open_id(open_id), &user_json, ttl).await {
                    tracing::warn!(error = ?e, "缓存用户信息失败 (open_id)");
                }
            }
            
            // 通过 name 缓存
            if let Err(e) = redis.set_with_ttl(&self.cache_key_name(&user.name), &user_json, ttl).await {
                tracing::warn!(error = ?e, "缓存用户信息失败 (name)");
            }
            
            // 通过 email 缓存
            if let Err(e) = redis.set_with_ttl(&self.cache_key_email(&user.email), &user_json, ttl).await {
                tracing::warn!(error = ?e, "缓存用户信息失败 (email)");
            }
            
            // 通过 phone 缓存（如果有）
            if let Some(ref phone) = user.phone {
                if let Err(e) = redis.set_with_ttl(&self.cache_key_phone(phone), &user_json, ttl).await {
                    tracing::warn!(error = ?e, "缓存用户信息失败 (phone)");
                }
            }
        }
        Ok(())
    }
    
    // 从缓存获取用户信息
    async fn get_user_from_cache(&self, key: &str) -> Result<Option<User>> {
        if let Some(ref redis) = self.redis {
            match redis.get(key).await {
                Ok(Some(user_json)) => {
                    match serde_json::from_str::<User>(&user_json) {
                        Ok(user) => {
                            tracing::debug!("从缓存获取用户信息: {}", key);
                            return Ok(Some(user));
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "反序列化用户信息失败");
                            // 缓存数据损坏，删除它
                            let _ = redis.del(key).await;
                        }
                    }
                }
                Ok(None) => {
                    // 缓存不存在
                }
                Err(e) => {
                    tracing::warn!(error = ?e, "从缓存获取用户信息失败");
                }
            }
        }
        Ok(None)
    }
    
    // 清除用户相关的所有缓存
    async fn invalidate_user_cache(&self, user: &User) {
        if let Some(ref redis) = self.redis {
            let mut keys = vec![
                self.cache_key_user_id(user.id),
                self.cache_key_name(&user.name),
                self.cache_key_email(&user.email),
            ];
            
            if let Some(ref open_id) = user.open_id {
                keys.push(self.cache_key_open_id(open_id));
                keys.push(self.cache_key_online_status(open_id));
            }
            
            if let Some(ref phone) = user.phone {
                keys.push(self.cache_key_phone(phone));
            }
            
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            if let Err(e) = redis.del_many(&key_refs).await {
                tracing::warn!(error = ?e, "清除用户缓存失败");
            } else {
                tracing::debug!("清除用户缓存: {:?}", keys);
            }
        }
    }
    

    pub async fn get_by_id(&self, id: u64) -> Result<User> {
        // 先从缓存获取
        if let Some(user) = self.get_user_from_cache(&self.cache_key_user_id(id)).await? {
            return Ok(user);
        }
        
        // 先检查用户是否存在
        let exists: Option<u64> = sqlx::query_scalar(
            "SELECT id FROM users WHERE id = ? AND (status IS NULL OR status = 1)"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            tracing::error!(user_id = %id, error = %e, "检查用户是否存在失败");
            ErrorCode::Database
        })?;
        
        if exists.is_none() {
            tracing::warn!(user_id = %id, "用户不存在或状态异常");
            return Err(ErrorCode::NotFound);
        }
        
        // 查询用户详细信息（不再查询 is_online 和 last_online_time，这些字段现在只存在 Redis 中）
        let user = sqlx::query_as::<_, User>(
            "SELECT id, open_id, name, email, password_hash, file_name, abstract as abstract_field, phone, status, gender 
             FROM users WHERE id = ? AND (status IS NULL OR status = 1)"
        )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| {
                tracing::error!(user_id = %id, error = %e, "查询用户详细信息失败（get_by_id）");
                ErrorCode::Database
            })?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_user(&u).await;
                Ok(u)
            },
            None => {
                tracing::warn!(user_id = %id, "用户详细信息查询返回空结果");
                Err(ErrorCode::NotFound)
            },
        }
    }
    
    
    /// 根据 open_id 获取用户
    pub async fn get_by_open_id(&self, open_id: &str) -> Result<User> {
        // 先从缓存获取
        if let Some(user) = self.get_user_from_cache(&self.cache_key_open_id(open_id)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, User>(
            "SELECT id, open_id, name, email, password_hash, file_name, abstract as abstract_field, phone, status, gender 
             FROM users WHERE open_id = ? AND (status IS NULL OR status = 1)"
        )
            .bind(open_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }

    pub async fn get_by_email(&self, email: &str) -> Result<User> {
        // 先从缓存获取
        if let Some(user) = self.get_user_from_cache(&self.cache_key_email(email)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, User>(
            "SELECT id, open_id, name, email, password_hash, file_name, abstract as abstract_field, phone, status, gender 
             FROM users WHERE email = ? AND (status IS NULL OR status = 1)"
        )
            .bind(email)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 根据用户名获取用户
    pub async fn get_by_name(&self, name: &str) -> Result<User> {
        // 先从缓存获取
        if let Some(user) = self.get_user_from_cache(&self.cache_key_name(name)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, User>(
            "SELECT id, open_id, name, email, password_hash, file_name, abstract as abstract_field, phone, status, gender 
             FROM users WHERE name = ? AND (status IS NULL OR status = 1)"
        )
            .bind(name)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }
    
    /// 根据手机号获取用户
    pub async fn get_by_phone(&self, phone: &str) -> Result<User> {
        // 先从缓存获取
        if let Some(user) = self.get_user_from_cache(&self.cache_key_phone(phone)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, User>(
            "SELECT id, open_id, name, email, password_hash, file_name, abstract as abstract_field, phone, status, gender 
             FROM users WHERE phone = ? AND (status IS NULL OR status = 1)"
        )
            .bind(phone)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 创建用户
    /// 自动生成 open_id（使用雪花算法，保证唯一性）
    /// open_id 在创建后不能修改
    pub async fn create(&self, name: String, email: String, password: String, phone: Option<String>) -> Result<User> {
        use tracing::warn;
        
        if name.is_empty() {
            return Err(ErrorCode::InvalidInput);
        }
        // 昵称长度检查（建议 2-20 个字符）
        if name.len() < 2 || name.len() > 20 {
            return Err(ErrorCode::InvalidInput);
        }
        if !email.contains('@') {
            return Err(ErrorCode::InvalidInput);
        }
        if password.len() < 6 {
            return Err(ErrorCode::InvalidInput);
        }

        // 检查昵称是否已被占用
        if self.get_by_name(&name).await.is_ok() {
            warn!("用户名已被占用: {}", name);
            return Err(ErrorCode::InvalidInput);
        }
        
        // 检查邮箱是否已存在
        if self.get_by_email(&email).await.is_ok() {
            warn!("邮箱已被占用: {}", email);
            return Err(ErrorCode::InvalidInput);
        }
        
        // 如果提供了手机号，检查是否已被占用
        if let Some(ref phone_num) = phone {
            if !phone_num.is_empty() {
                // 简单的手机号格式验证（11位数字）
                if phone_num.len() != 11 || !phone_num.chars().all(|c| c.is_ascii_digit()) {
                    warn!("手机号格式不正确: {}", phone_num);
                    return Err(ErrorCode::InvalidInput);
                }
                
                // 检查手机号是否已被占用
                if self.get_by_phone(phone_num).await.is_ok() {
                    warn!("手机号已被占用: {}", phone_num);
                    return Err(ErrorCode::InvalidInput);
                }
            }
        }

        // 使用雪花算法生成 open_id，保证唯一性
        // open_id 在创建后不能修改
        let open_id = generate_snowflake_id().to_string();
        
        // 加密密码
        let password_hash = hash(password, DEFAULT_COST)
            .map_err(|_| ErrorCode::Internal)?;

        let result = sqlx::query(
            "INSERT INTO users (open_id, name, email, password_hash, phone, status, gender) VALUES (?, ?, ?, ?, ?, 1, 3)"
        )
        .bind(&open_id)
        .bind(&name)
        .bind(&email)
        .bind(&password_hash)
        .bind(&phone)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let user_id = result.last_insert_id();
        let user = User::new(user_id, Some(open_id), name, email);
        
        // 缓存新创建的用户
        let _ = self.cache_user(&user).await;
        
        Ok(user)
    }
    
    /// 确保用户有 open_id，如果没有则生成一个
    /// 注意：这仅用于兼容旧数据，新创建的用户必须包含 open_id
    /// 使用雪花算法生成 open_id，保证唯一性
    pub async fn ensure_open_id(&self, user_id: u64) -> Result<String> {
        let user = self.get_by_id(user_id).await?;
        
        if let Some(ref open_id) = user.open_id {
            return Ok(open_id.clone());
        }
        
        // 生成新的 open_id（仅用于兼容旧数据）
        // 使用雪花算法生成 open_id
        let open_id = generate_snowflake_id().to_string();
        sqlx::query("UPDATE users SET open_id = ? WHERE id = ? AND open_id IS NULL")
            .bind(&open_id)
            .bind(user_id)
            .execute(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;
        
        // 清除缓存，下次查询时会重新加载
        self.invalidate_user_cache(&user).await;
        
        Ok(open_id)
    }
    
    /// 更新用户信息（简化版本，分别更新每个字段）
    /// 注意：open_id 不能通过此方法更新，它只能在创建时生成
    pub async fn update_user(&self, user_id: u64, name: Option<String>, file_name: Option<String>, abstract_field: Option<String>, phone: Option<String>, gender: Option<i8>) -> Result<User> {
        if let Some(n) = name {
            // 昵称长度检查
            if n.len() < 2 || n.len() > 20 {
                return Err(ErrorCode::InvalidInput);
            }
            // 检查昵称是否已被其他用户占用（排除当前用户）
            match self.get_by_name(&n).await {
                Ok(existing_user) => {
                    // 如果昵称被其他用户占用，返回错误
                    if existing_user.id != user_id {
                        return Err(ErrorCode::InvalidInput);
                    }
                    // 如果昵称就是当前用户的，允许更新（可能是其他字段的更新）
                }
                Err(ErrorCode::NotFound) => {
                    // 昵称未被占用，可以更新
                }
                Err(e) => return Err(e),
            }
            
            sqlx::query("UPDATE users SET name = ? WHERE id = ?")
                .bind(&n)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(|_| ErrorCode::Database)?;
        }
        if let Some(f) = file_name {
            // 如果 file_name 是空字符串，设置为 NULL
            if f.is_empty() {
                sqlx::query("UPDATE users SET file_name = NULL WHERE id = ?")
                    .bind(user_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|_| ErrorCode::Database)?;
            } else {
                sqlx::query("UPDATE users SET file_name = ? WHERE id = ?")
                    .bind(&f)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|_| ErrorCode::Database)?;
            }
        }
        if let Some(a) = abstract_field {
            // 如果 abstract_field 是空字符串，设置为 NULL
            if a.is_empty() {
                sqlx::query("UPDATE users SET abstract = NULL WHERE id = ?")
                    .bind(user_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|_| ErrorCode::Database)?;
            } else {
                sqlx::query("UPDATE users SET abstract = ? WHERE id = ?")
                    .bind(&a)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|_| ErrorCode::Database)?;
            }
        }
        if let Some(p) = phone {
            // 如果手机号不为空，检查是否已被其他用户占用
            if !p.is_empty() {
                match self.get_by_phone(&p).await {
                    Ok(existing_user) => {
                        // 如果手机号被其他用户占用，返回错误
                        if existing_user.id != user_id {
                            return Err(ErrorCode::InvalidInput);
                        }
                        // 如果手机号就是当前用户的，允许更新（可能是其他字段的更新）
                    }
                    Err(ErrorCode::NotFound) => {
                        // 手机号未被占用，可以更新
                    }
                    Err(e) => return Err(e),
                }
            }
            
            // 如果手机号是空字符串，设置为 NULL
            if p.is_empty() {
                sqlx::query("UPDATE users SET phone = NULL WHERE id = ?")
                    .bind(user_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|_| ErrorCode::Database)?;
            } else {
                sqlx::query("UPDATE users SET phone = ? WHERE id = ?")
                    .bind(&p)
                    .bind(user_id)
                    .execute(&self.pool)
                    .await
                    .map_err(|_| ErrorCode::Database)?;
            }
        }
        if let Some(g) = gender {
            sqlx::query("UPDATE users SET gender = ? WHERE id = ?")
                .bind(g)
                .bind(user_id)
                .execute(&self.pool)
                .await
                .map_err(|_| ErrorCode::Database)?;
        }
        
        // 获取更新后的用户信息
        let updated_user = self.get_by_id(user_id).await?;
        
        // 清除旧缓存（因为可能更新了 name、email 等字段）
        // 注意：这里需要先获取旧用户信息来清除旧键
        // 但由于我们已经更新了，直接清除所有可能的键
        self.invalidate_user_cache(&updated_user).await;
        
        // 重新缓存更新后的用户信息
        let _ = self.cache_user(&updated_user).await;
        
        Ok(updated_user)
    }

    pub async fn verify_password(&self, username: &str, password: &str) -> Result<User> {
        // 支持用户名或邮箱登录
        // 如果包含 @ 符号，认为是邮箱，否则认为是用户名
        let user = if username.contains('@') {
            self.get_by_email(username).await?
        } else {
            self.get_by_name(username).await?
        };
        
        match &user.password_hash {
            Some(hash) => {
                if verify(password, hash).map_err(|_| ErrorCode::Internal)? {
                    Ok(user)
                } else {
                    Err(ErrorCode::Unauthorized)
                }
            }
            None => Err(ErrorCode::Unauthorized),
        }
    }
}