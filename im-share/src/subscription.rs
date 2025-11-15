use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use uuid::Uuid;

/// 订阅 ID 管理服务
/// 维护用户 ID 和订阅 ID 的映射关系
#[derive(Clone)]
pub struct SubscriptionService {
    // 订阅 ID -> 用户 ID
    subscriptions: Arc<RwLock<HashMap<String, u64>>>,
    // 用户 ID -> 订阅 ID 列表（一个用户可以有多个设备）
    user_subscriptions: Arc<RwLock<HashMap<u64, Vec<String>>>>,
}

impl SubscriptionService {
    pub fn new() -> Self {
        Self {
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            user_subscriptions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 为用户生成或获取订阅 ID
    /// 如果用户已有订阅 ID，返回现有的；否则生成新的
    pub fn get_or_create_subscription_id(&self, user_id: u64) -> String {
        let mut user_subs = self.user_subscriptions.write().unwrap();
        
        // 如果用户已有订阅 ID，返回第一个
        if let Some(subs) = user_subs.get(&user_id) {
            if !subs.is_empty() {
                return subs[0].clone();
            }
        }

        // 生成新的订阅 ID
        let subscription_id = format!("sub_{}", Uuid::new_v4().to_string().replace("-", ""));
        
        // 更新映射关系
        let mut subs = self.subscriptions.write().unwrap();
        subs.insert(subscription_id.clone(), user_id);
        
        user_subs.entry(user_id).or_insert_with(Vec::new).push(subscription_id.clone());
        
        subscription_id
    }

    /// 创建新的订阅 ID（允许多设备登录）
    pub fn create_subscription_id(&self, user_id: u64) -> String {
        let subscription_id = format!("sub_{}", Uuid::new_v4().to_string().replace("-", ""));
        
        let mut subs = self.subscriptions.write().unwrap();
        subs.insert(subscription_id.clone(), user_id);
        
        let mut user_subs = self.user_subscriptions.write().unwrap();
        user_subs.entry(user_id).or_insert_with(Vec::new).push(subscription_id.clone());
        
        subscription_id
    }

    /// 根据订阅 ID 获取用户 ID
    pub fn get_user_id(&self, subscription_id: &str) -> Option<u64> {
        let subs = self.subscriptions.read().unwrap();
        subs.get(subscription_id).copied()
    }

    /// 根据用户 ID 获取所有订阅 ID
    pub fn get_subscription_ids(&self, user_id: u64) -> Vec<String> {
        let user_subs = self.user_subscriptions.read().unwrap();
        user_subs.get(&user_id).cloned().unwrap_or_default()
    }

    /// 删除订阅 ID
    pub fn remove_subscription(&self, subscription_id: &str) {
        let mut subs = self.subscriptions.write().unwrap();
        if let Some(user_id) = subs.remove(subscription_id) {
            let mut user_subs = self.user_subscriptions.write().unwrap();
            if let Some(subs_list) = user_subs.get_mut(&user_id) {
                subs_list.retain(|s| s != subscription_id);
                if subs_list.is_empty() {
                    user_subs.remove(&user_id);
                }
            }
        }
    }

    /// 删除用户的所有订阅
    pub fn remove_user_subscriptions(&self, user_id: u64) {
        let mut user_subs = self.user_subscriptions.write().unwrap();
        if let Some(subs) = user_subs.remove(&user_id) {
            let mut subscriptions = self.subscriptions.write().unwrap();
            for sub_id in subs {
                subscriptions.remove(&sub_id);
            }
        }
    }

    /// 手动添加订阅 ID（用于从数据库同步到内存）
    pub fn add_subscription_id(&self, subscription_id: String, user_id: u64) {
        let mut subs = self.subscriptions.write().unwrap();
        subs.insert(subscription_id.clone(), user_id);
        
        let mut user_subs = self.user_subscriptions.write().unwrap();
        let user_sub_list = user_subs.entry(user_id).or_insert_with(Vec::new);
        if !user_sub_list.contains(&subscription_id) {
            user_sub_list.push(subscription_id);
        }
    }
}

impl Default for SubscriptionService {
    fn default() -> Self {
        Self::new()
    }
}

/// 根据订阅 ID 从服务器查询用户 ID（返回snowflake_id用于MQTT）
/// 这是一个客户端函数，用于 im-connect 服务查询用户 ID
pub async fn get_user_id_by_subscription(server_url: &str, subscription_id: &str) -> anyhow::Result<u64> {
    let url = format!("{}/api/subscriptions/{}/user", server_url, subscription_id);
    let response = reqwest::get(&url).await?;
    
    if !response.status().is_success() {
        anyhow::bail!("HTTP 状态码: {}", response.status());
    }
    
    let json: serde_json::Value = response.json().await?;
    
    // 优先使用snowflake_id，如果没有则使用user_id（向后兼容）
    if let Some(snowflake_id) = json.get("snowflake_id").and_then(|v| v.as_u64()) {
        Ok(snowflake_id)
    } else {
        json.get("user_id")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow::anyhow!("用户 ID 不存在"))
    }
}

/// 根据订阅 ID 从服务器查询用户信息（返回snowflake_id和open_id）
/// 这是一个客户端函数，用于 im-connect 服务查询用户信息
/// 返回 (snowflake_id, open_id) 元组
/// 包含重试机制，最多重试3次
pub async fn get_user_info_by_subscription(server_url: &str, subscription_id: &str) -> anyhow::Result<(u64, String)> {
    use tracing::{warn, error};
    
    let url = format!("{}/api/subscriptions/{}/user", server_url, subscription_id);
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY_MS: u64 = 1000;
    
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|e| anyhow::anyhow!("创建 HTTP 客户端失败: {}", e))?;
    
    let mut last_error = None;
    
    for attempt in 1..=MAX_RETRIES {
        match client.get(&url).send().await {
            Ok(response) => {
                if !response.status().is_success() {
                    let status = response.status();
                    let error_text = response.text().await.unwrap_or_else(|_| "未知错误".to_string());
                    
                    if attempt < MAX_RETRIES {
                        warn!(
                            url = %url, 
                            status = %status, 
                            attempt = attempt,
                            max_retries = MAX_RETRIES,
                            "im-server API 返回错误，将重试"
                        );
                        tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS * attempt as u64)).await;
                        continue;
                    } else {
                        error!(
                            url = %url, 
                            status = %status, 
                            error = %error_text,
                            "im-server API 返回错误，已达到最大重试次数"
                        );
                        anyhow::bail!("HTTP 状态码: {}, 错误: {}", status, error_text);
                    }
                }
                
                let json: serde_json::Value = response.json().await?;
                
                // 获取 user_id（数据库ID）
                let user_id = json.get("user_id")
                    .and_then(|v| v.as_u64())
                    .ok_or_else(|| anyhow::anyhow!("用户 ID 不存在"))?;
                
                // 获取 snowflake_id（用于 MQTT）
                let snowflake_id = if let Some(snowflake_id) = json.get("snowflake_id").and_then(|v| v.as_u64()) {
                    snowflake_id
                } else {
                    user_id
                };
                
                // 获取 open_id（用于 Redis 离线消息）
                // 如果 open_id 不存在，使用 user_id 作为后备（与存储时的逻辑一致）
                // 存储时：user.open_id.clone().unwrap_or_else(|| user.id.to_string())
                let open_id = json.get("open_id")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| user_id.to_string()); // 如果没有 open_id，使用 user_id 的字符串形式作为后备
                
                return Ok((snowflake_id, open_id));
            }
            Err(e) => {
                last_error = Some(e);
                
                if attempt < MAX_RETRIES {
                    warn!(
                        url = %url, 
                        error = %last_error.as_ref().unwrap(),
                        attempt = attempt,
                        max_retries = MAX_RETRIES,
                        "连接 im-server 失败，将重试。请确保 im-server 正在运行在 {}",
                        server_url
                    );
                    tokio::time::sleep(std::time::Duration::from_millis(RETRY_DELAY_MS * attempt as u64)).await;
                } else {
                    error!(
                        url = %url, 
                        error = %last_error.as_ref().unwrap(),
                        "连接 im-server 失败，已达到最大重试次数。请检查：\n  1. im-server 是否正在运行？\n  2. im-server 是否运行在 {}？\n  3. 网络连接是否正常？",
                        server_url
                    );
                }
            }
        }
    }
    
    // 如果所有重试都失败了
    anyhow::bail!(
        "连接 im-server 失败: {} (URL: {}). 请确保 im-server 正在运行在 {}",
        last_error.unwrap(),
        url,
        server_url
    )
}

