use crate::model::{ImUser, ImUserData};
use crate::error::{ErrorCode, Result};
use im_share::RedisClient;
use sqlx::MySqlPool;
use bcrypt::{hash, verify, DEFAULT_COST};
use im_share::now_timestamp;
use std::sync::Arc;
use serde_json;

pub struct ImUserService {
    pool: MySqlPool,
    redis: Option<Arc<RedisClient>>,
}

impl ImUserService {
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
    fn cache_key_user_id(&self, user_id: &str) -> String {
        format!("im_user:user_id:{}", user_id)
    }
    
    fn cache_key_user_name(&self, user_name: &str) -> String {
        format!("im_user:user_name:{}", user_name)
    }
    
    fn cache_key_mobile(&self, mobile: &str) -> String {
        format!("im_user:mobile:{}", mobile)
    }
    
    fn cache_key_user_data(&self, user_id: &str) -> String {
        format!("im_user_data:{}", user_id)
    }
    
    // 缓存 ImUser 信息（TTL: 1小时）
    async fn cache_im_user(&self, user: &ImUser) -> Result<()> {
        if let Some(ref redis) = self.redis {
            let user_json = serde_json::to_string(user)
                .map_err(|_| ErrorCode::Internal)?;
            
            let ttl = 3600u64; // 1小时
            
            // 通过 user_id 缓存
            if let Err(e) = redis.set_with_ttl(&self.cache_key_user_id(&user.user_id), &user_json, ttl).await {
                tracing::warn!(error = ?e, "缓存 ImUser 信息失败 (user_id)");
            }
            
            // 通过 user_name 缓存
            if let Some(ref user_name) = user.user_name {
                if let Err(e) = redis.set_with_ttl(&self.cache_key_user_name(user_name), &user_json, ttl).await {
                    tracing::warn!(error = ?e, "缓存 ImUser 信息失败 (user_name)");
                }
            }
            
            // 通过 mobile 缓存（如果有）
            if let Some(ref mobile) = user.mobile {
                if let Err(e) = redis.set_with_ttl(&self.cache_key_mobile(mobile), &user_json, ttl).await {
                    tracing::warn!(error = ?e, "缓存 ImUser 信息失败 (mobile)");
                }
            }
        }
        Ok(())
    }
    
    // 从缓存获取 ImUser 信息
    async fn get_im_user_from_cache(&self, key: &str) -> Result<Option<ImUser>> {
        if let Some(ref redis) = self.redis {
            match redis.get(key).await {
                Ok(Some(user_json)) => {
                    match serde_json::from_str::<ImUser>(&user_json) {
                        Ok(user) => {
                            tracing::debug!("从缓存获取 ImUser 信息: {}", key);
                            return Ok(Some(user));
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "反序列化 ImUser 信息失败");
                            let _ = redis.del(key).await;
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = ?e, "从缓存获取 ImUser 信息失败");
                }
            }
        }
        Ok(None)
    }
    
    // 清除 ImUser 相关的所有缓存
    #[allow(dead_code)]
    async fn invalidate_im_user_cache(&self, user: &ImUser) {
        if let Some(ref redis) = self.redis {
            let mut keys = vec![self.cache_key_user_id(&user.user_id)];
            
            if let Some(ref user_name) = user.user_name {
                keys.push(self.cache_key_user_name(user_name));
            }
            
            if let Some(ref mobile) = user.mobile {
                keys.push(self.cache_key_mobile(mobile));
            }
            
            let key_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
            if let Err(e) = redis.del_many(&key_refs).await {
                tracing::warn!(error = ?e, "清除 ImUser 缓存失败");
            }
        }
    }


    /// 根据user_id获取用户（user_id 对应 users.open_id）
    pub async fn get_by_user_id(&self, user_id: &str) -> Result<ImUser> {
        // 先从缓存获取
        if let Some(user) = self.get_im_user_from_cache(&self.cache_key_user_id(user_id)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, ImUser>(
            "SELECT open_id as user_id, name as user_name, password_hash as password, 
                    phone as mobile, create_time, update_time, version, del_flag 
             FROM users 
             WHERE open_id = ? AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_im_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 根据用户名获取用户
    pub async fn get_by_user_name(&self, user_name: &str) -> Result<ImUser> {
        // 先从缓存获取
        if let Some(user) = self.get_im_user_from_cache(&self.cache_key_user_name(user_name)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, ImUser>(
            "SELECT open_id as user_id, name as user_name, password_hash as password, 
                    phone as mobile, create_time, update_time, version, del_flag 
             FROM users 
             WHERE name = ? AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(user_name)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_im_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 根据手机号获取用户
    #[allow(dead_code)]
    pub async fn get_by_mobile(&self, mobile: &str) -> Result<ImUser> {
        // 先从缓存获取
        if let Some(user) = self.get_im_user_from_cache(&self.cache_key_mobile(mobile)).await? {
            return Ok(user);
        }
        
        let user = sqlx::query_as::<_, ImUser>(
            "SELECT open_id as user_id, name as user_name, password_hash as password, 
                    phone as mobile, create_time, update_time, version, del_flag 
             FROM users 
             WHERE phone = ? AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(mobile)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        match user {
            Some(u) => {
                // 缓存用户信息
                let _ = self.cache_im_user(&u).await;
                Ok(u)
            },
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 创建用户
    pub async fn create(&self, user_id: String, user_name: String, password: String, mobile: Option<String>) -> Result<ImUser> {
        if user_name.is_empty() {
            return Err(ErrorCode::InvalidInput);
        }
        if password.len() < 6 {
            return Err(ErrorCode::InvalidInput);
        }

        // 检查用户名是否已存在
        if self.get_by_user_name(&user_name).await.is_ok() {
            return Err(ErrorCode::InvalidInput);
        }

        // 加密密码
        let password_hash = hash(password, DEFAULT_COST)
            .map_err(|_| ErrorCode::Internal)?;

        let now = now_timestamp();

        // 检查 open_id 是否已存在（user_id 对应 open_id）
        let existing = sqlx::query("SELECT id FROM users WHERE open_id = ?")
            .bind(&user_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;
        
        if existing.is_some() {
            return Err(ErrorCode::InvalidInput);
        }

        // 插入到 users 表，使用 open_id 作为 user_id
        // 注意：users 表需要 email 字段，这里使用 user_id@im.local 作为临时 email
        let email = format!("{}@im.local", user_id);
        sqlx::query(
            "INSERT INTO users (open_id, name, email, password_hash, phone, create_time, update_time, version, del_flag, status) 
             VALUES (?, ?, ?, ?, ?, ?, ?, 1, 1, 1)"
        )
        .bind(&user_id)
        .bind(&user_name)
        .bind(&email)
        .bind(&password_hash)
        .bind(&mobile)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        let new_user = ImUser {
            user_id: user_id.clone(),
            user_name: Some(user_name),
            password: Some(password_hash),
            mobile,
            create_time: Some(now),
            update_time: Some(now),
            version: Some(1),
            del_flag: Some(1),
        };
        
        // 自动创建对应的 im_user_data 记录
        let default_user_data = ImUserData {
            user_id: user_id.clone(),
            name: None,
            avatar: None,
            gender: None,
            birthday: None,
            location: None,
            self_signature: None,
            friend_allow_type: 1, // 默认允许任何人添加好友
            forbidden_flag: 0,    // 默认未封禁
            disable_add_friend: 0, // 默认允许添加好友
            silent_flag: 0,        // 默认未静音
            user_type: 1,          // 默认普通用户
            del_flag: 1,            // 默认未删除
            extra: None,
            create_time: Some(now),
            update_time: Some(now),
            version: Some(1),
        };
        
        // 创建用户数据记录
        if let Err(e) = self.create_user_data_internal(default_user_data, now).await {
            tracing::warn!(error = ?e, user_id = %user_id, "创建用户数据记录失败，但用户已创建");
        }
        
        // 缓存新创建的用户
        let _ = self.cache_im_user(&new_user).await;
        
        Ok(new_user)
    }

    /// 验证密码
    pub async fn verify_password(&self, user_name: &str, password: &str) -> Result<ImUser> {
        let user = self.get_by_user_name(user_name).await?;
        
        match &user.password {
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

    /// 获取用户数据
    pub async fn get_user_data(&self, user_id: &str) -> Result<ImUserData> {
        // 先从缓存获取
        if let Some(ref redis) = self.redis {
            match redis.get(&self.cache_key_user_data(user_id)).await {
                Ok(Some(data_json)) => {
                    match serde_json::from_str::<ImUserData>(&data_json) {
                        Ok(data) => {
                            tracing::debug!("从缓存获取 ImUserData: {}", user_id);
                            return Ok(data);
                        }
                        Err(e) => {
                            tracing::warn!(error = ?e, "反序列化 ImUserData 失败");
                            let _ = redis.del(&self.cache_key_user_data(user_id)).await;
                        }
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    tracing::warn!(error = ?e, "从缓存获取 ImUserData 失败");
                }
            }
        }
        
        let user_data = sqlx::query_as::<_, ImUserData>(
            "SELECT user_id, name, avatar, gender, birthday, location, self_signature, 
                    friend_allow_type, forbidden_flag, disable_add_friend, silent_flag, 
                    user_type, del_flag, extra, create_time, update_time, version 
             FROM im_user_data 
             WHERE user_id = ? AND del_flag = 1"
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        match user_data {
            Some(u) => {
                // 缓存用户数据（TTL: 1小时）
                if let Some(ref redis) = self.redis {
                    if let Ok(data_json) = serde_json::to_string(&u) {
                        let _ = redis.set_with_ttl(&self.cache_key_user_data(user_id), &data_json, 3600).await;
                    }
                }
                Ok(u)
            },
            None => {
                // 如果用户数据不存在，检查用户是否存在，如果存在则自动创建默认数据
                if let Ok(_user) = self.get_by_user_id(user_id).await {
                    let now = now_timestamp();
                    let default_user_data = ImUserData {
                        user_id: user_id.to_string(),
                        name: None,
                        avatar: None,
                        gender: None,
                        birthday: None,
                        location: None,
                        self_signature: None,
                        friend_allow_type: 1,
                        forbidden_flag: 0,
                        disable_add_friend: 0,
                        silent_flag: 0,
                        user_type: 1,
                        del_flag: 1,
                        extra: None,
                        create_time: Some(now),
                        update_time: Some(now),
                        version: Some(1),
                    };
                    
                    // 创建默认用户数据
                    if let Err(e) = self.create_user_data_internal(default_user_data.clone(), now).await {
                        tracing::warn!(error = ?e, user_id = %user_id, "自动创建用户数据失败");
                        return Err(ErrorCode::NotFound);
                    }
                    
                    // 缓存新创建的用户数据
                    if let Some(ref redis) = self.redis {
                        if let Ok(data_json) = serde_json::to_string(&default_user_data) {
                            let _ = redis.set_with_ttl(&self.cache_key_user_data(user_id), &data_json, 3600).await;
                        }
                    }
                    
                    Ok(default_user_data)
                } else {
                    Err(ErrorCode::NotFound)
                }
            },
        }
    }

    /// 内部方法：创建用户数据记录
    async fn create_user_data_internal(&self, user_data: ImUserData, now: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO im_user_data 
             (user_id, name, avatar, gender, birthday, location, self_signature, 
              friend_allow_type, forbidden_flag, disable_add_friend, silent_flag, 
              user_type, del_flag, extra, create_time, update_time, version) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)"
        )
        .bind(&user_data.user_id)
        .bind(&user_data.name)
        .bind(&user_data.avatar)
        .bind(&user_data.gender)
        .bind(&user_data.birthday)
        .bind(&user_data.location)
        .bind(&user_data.self_signature)
        .bind(user_data.friend_allow_type)
        .bind(user_data.forbidden_flag)
        .bind(user_data.disable_add_friend)
        .bind(user_data.silent_flag)
        .bind(user_data.user_type)
        .bind(user_data.del_flag)
        .bind(&user_data.extra)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 创建或更新用户数据
    pub async fn upsert_user_data(&self, user_data: ImUserData) -> Result<()> {
        let now = now_timestamp();
        
        sqlx::query(
            "INSERT INTO im_user_data 
             (user_id, name, avatar, gender, birthday, location, self_signature, 
              friend_allow_type, forbidden_flag, disable_add_friend, silent_flag, 
              user_type, del_flag, extra, create_time, update_time, version) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1)
             ON DUPLICATE KEY UPDATE 
             name = VALUES(name), avatar = VALUES(avatar), gender = VALUES(gender), 
             birthday = VALUES(birthday), location = VALUES(location), 
             self_signature = VALUES(self_signature), 
             friend_allow_type = VALUES(friend_allow_type), 
             forbidden_flag = VALUES(forbidden_flag), 
             disable_add_friend = VALUES(disable_add_friend), 
             silent_flag = VALUES(silent_flag), user_type = VALUES(user_type), 
             extra = VALUES(extra), update_time = ?, version = version + 1"
        )
        .bind(&user_data.user_id)
        .bind(&user_data.name)
        .bind(&user_data.avatar)
        .bind(&user_data.gender)
        .bind(&user_data.birthday)
        .bind(&user_data.location)
        .bind(&user_data.self_signature)
        .bind(user_data.friend_allow_type)
        .bind(user_data.forbidden_flag)
        .bind(user_data.disable_add_friend)
        .bind(user_data.silent_flag)
        .bind(user_data.user_type)
        .bind(user_data.del_flag)
        .bind(&user_data.extra)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        // 清除用户数据缓存
        if let Some(ref redis) = self.redis {
            let _ = redis.del(&self.cache_key_user_data(&user_data.user_id)).await;
        }

        Ok(())
    }
}

