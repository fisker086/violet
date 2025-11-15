use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde_json::Value;
use tracing::warn;
use once_cell::sync::Lazy;

// 用户名缓存（ID -> 用户名）
static USERNAME_CACHE: Lazy<Arc<RwLock<HashMap<String, String>>>> = 
    Lazy::new(|| Arc::new(RwLock::new(HashMap::new())));

/// 根据用户ID（雪花ID、数据库ID、open_id或用户名）获取用户名
/// 
/// 这个函数会：
/// 1. 先检查缓存
/// 2. 尝试通过 IM API（查询 im_user 表）获取用户名
/// 3. 如果失败，尝试通过 users API（查询 users 表）获取用户名
/// 4. 如果都失败，返回原始ID
/// 
/// # Arguments
/// * `server_url` - im-server 的 URL（例如：http://127.0.0.1:3000）
/// * `user_id` - 用户ID（可以是雪花ID、数据库ID、open_id或用户名）
/// 
/// # Returns
/// * `Ok(String)` - 用户名
/// * `Err(String)` - 错误信息（实际上总是返回 Ok，失败时返回原始ID）
pub async fn get_username_by_id(server_url: &str, user_id: &str) -> Result<String, String> {
    // 先检查缓存
    {
        let cache = USERNAME_CACHE.read().await;
        if let Some(username) = cache.get(user_id) {
            return Ok(username.clone());
        }
    }
    
    use reqwest::Client;
    
    let client = Client::new();
    
    // 先尝试使用不需要认证的 IM API（查询 im_user 表）
    let im_url = format!("{}/api/im/users/{}", server_url, urlencoding::encode(user_id));
    
    // 如果 IM API 成功，使用结果
    match client.get(&im_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<Value>().await {
                    Ok(user_json) => {
                        // 尝试获取 user_name 字段（IM API 返回的字段名）
                        if let Some(name) = user_json.get("user_name").and_then(|v| v.as_str()) {
                            let username = name.to_string();
                            // 更新缓存
                            {
                                let mut cache = USERNAME_CACHE.write().await;
                                cache.insert(user_id.to_string(), username.clone());
                            }
                            return Ok(username);
                        } else if let Some(name) = user_json.get("name").and_then(|v| v.as_str()) {
                            let username = name.to_string();
                            // 更新缓存
                            {
                                let mut cache = USERNAME_CACHE.write().await;
                                cache.insert(user_id.to_string(), username.clone());
                            }
                            return Ok(username);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, user_id = %user_id, "解析IM用户信息失败");
                    }
                }
            }
        }
        Err(e) => {
            warn!(error = %e, user_id = %user_id, "请求IM用户信息失败");
        }
    }
    
    // 如果 IM API 失败（404），尝试使用 users API（查询 users 表，支持雪花ID）
    // 使用不需要认证的内部API：/api/users/{id}/name
    let users_url = format!("{}/api/users/{}/name", server_url, urlencoding::encode(user_id));
    match client.get(&users_url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<Value>().await {
                    Ok(user_json) => {
                        // 尝试获取 name 字段
                        if let Some(name) = user_json.get("name").and_then(|v| v.as_str()) {
                            let username = name.to_string();
                            // 更新缓存
                            {
                                let mut cache = USERNAME_CACHE.write().await;
                                cache.insert(user_id.to_string(), username.clone());
                            }
                            return Ok(username);
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, user_id = %user_id, "解析用户信息失败");
                    }
                }
            } else if response.status() != reqwest::StatusCode::NOT_FOUND {
                // 如果不是404，记录警告
                warn!(status = %response.status(), user_id = %user_id, "获取用户信息失败（非404）");
            }
        }
        Err(e) => {
            warn!(error = %e, user_id = %user_id, "请求用户信息失败");
        }
    }
    
    // 如果所有API调用都失败，返回原始ID（不缓存错误结果）
    Ok(user_id.to_string())
}

/// 清除用户名缓存（用于测试或缓存失效）
pub async fn clear_username_cache() {
    let mut cache = USERNAME_CACHE.write().await;
    cache.clear();
}

/// 获取缓存大小（用于监控）
pub async fn get_cache_size() -> usize {
    let cache = USERNAME_CACHE.read().await;
    cache.len()
}

/// 根据 open_id 或用户名从服务器查询用户的 open_id 数字形式（用于MQTT）
/// 这是一个客户端函数，用于 im-connect 服务查询用户的数字ID
/// 
/// # Arguments
/// * `server_url` - im-server 的 URL（例如：http://127.0.0.1:3000）
/// * `user_identifier` - 用户标识符（可以是 open_id、用户名）
/// 
/// # Returns
/// * `Ok(u64)` - 用户的 open_id 数字形式（用于MQTT）
/// * `Err(String)` - 错误信息
pub async fn get_snowflake_id_by_identifier(server_url: &str, user_identifier: &str) -> anyhow::Result<u64> {
    use reqwest::Client;
    
    let client = Client::new();
    
    // 如果已经是数字，直接返回（可能是 open_id 的数字形式）
    if let Ok(open_id_number) = user_identifier.parse::<u64>() {
        return Ok(open_id_number);
    }
    
    // 通过 im-server 的 API 查询用户信息
    // 使用不需要认证的内部 API：/api/users/{id}/snowflake_id
    // 这个 API 支持 open_id、用户名
    let url = format!("{}/api/users/{}/snowflake_id", server_url, urlencoding::encode(user_identifier));
    
    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(user_json) => {
                        // 获取 snowflake_id（实际上是 open_id 的数字形式，保持字段名兼容）
                        if let Some(snowflake_id) = user_json.get("snowflake_id").and_then(|v| v.as_u64()) {
                            return Ok(snowflake_id);
                        }
                        anyhow::bail!("用户信息中没有数字ID");
                    }
                    Err(e) => {
                        anyhow::bail!("解析用户信息失败: {}", e);
                    }
                }
            } else {
                anyhow::bail!("HTTP 状态码: {}", response.status());
            }
        }
        Err(e) => {
            anyhow::bail!("请求用户信息失败: {}", e);
        }
    }
}

/// 根据 open_id、用户名或 snowflake_id 从服务器查询用户的 open_id 字符串
/// 这是一个客户端函数，用于查询用户的 open_id（用于 Redis 在线状态）
/// 
/// # Arguments
/// * `server_url` - im-server 的 URL（例如：http://127.0.0.1:3000）
/// * `user_identifier` - 用户标识符（可以是 open_id、用户名或 snowflake_id）
/// 
/// # Returns
/// * `Ok(String)` - 用户的 open_id 字符串
/// * `Err` - 错误信息
pub async fn get_open_id_by_identifier(server_url: &str, user_identifier: &str) -> anyhow::Result<String> {
    use reqwest::Client;
    
    let client = Client::new();
    
    // 如果已经是数字字符串，可能是 open_id，先尝试直接使用
    // 但需要验证它是否是有效的 open_id
    if let Ok(_open_id_number) = user_identifier.parse::<u64>() {
        // 尝试通过 open_id 查询用户，验证它是否是有效的 open_id
        let test_url = format!("{}/api/users/{}/snowflake_id", server_url, user_identifier);
        if let Ok(test_response) = client.get(&test_url).send().await {
            if test_response.status().is_success() {
                // 如果查询成功，说明这是有效的 open_id，直接返回
                return Ok(user_identifier.to_string());
            }
        }
        // 如果查询失败，继续下面的逻辑
    }
    
    // 通过 im-server 的 API 查询用户信息
    // 使用不需要认证的内部 API：/api/users/{id}/snowflake_id
    // 这个 API 支持 open_id、用户名，返回 { "snowflake_id": ..., "open_id": ... }
    // 注意：返回的 "open_id" 字段实际上是 get_external_id()，可能不是真正的 open_id
    let url = format!("{}/api/users/{}/snowflake_id", server_url, urlencoding::encode(user_identifier));
    
    match client.get(&url).send().await {
        Ok(response) => {
            if response.status().is_success() {
                match response.json::<serde_json::Value>().await {
                    Ok(user_json) => {
                        // 获取返回的 open_id 字段（可能是 get_external_id()，即用户名）
                        let returned_open_id = user_json.get("open_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        
                        // 如果返回的 open_id 是数字字符串（长度较长，可能是真正的 open_id），使用它
                        if let Some(ref oid) = returned_open_id {
                            if oid.parse::<u64>().is_ok() && oid.len() > 10 {
                                // 看起来是真正的 open_id（数字字符串且长度较长）
                                return Ok(oid.clone());
                            }
                        }
                        
                        // 如果返回的 open_id 看起来不像真正的 open_id（可能是用户名），
                        // 尝试使用 snowflake_id 的字符串形式
                        // 但 snowflake_id 可能是数据库 ID，不是 open_id
                        // 所以我们需要通过其他方式获取真正的 open_id
                        
                        // 获取 snowflake_id（用于后续查询）
                        if let Some(snowflake_id) = user_json.get("snowflake_id").and_then(|v| v.as_u64()) {
                            // 如果 snowflake_id 和传入的 user_identifier 相同，说明传入的就是 open_id
                            if user_identifier.parse::<u64>().is_ok() 
                                && user_identifier.parse::<u64>().unwrap() == snowflake_id {
                                return Ok(user_identifier.to_string());
                            }
                            
                            // 否则，尝试通过 snowflake_id 查询真正的 open_id
                            // 但 snowflake_id 可能是数据库 ID，我们需要通过其他方式获取
                            // 暂时使用 snowflake_id 的字符串形式作为后备
                            return Ok(snowflake_id.to_string());
                        }
                        
                        anyhow::bail!("用户信息中没有 open_id 或 snowflake_id");
                    }
                    Err(e) => {
                        anyhow::bail!("解析用户信息失败: {}", e);
                    }
                }
            } else {
                anyhow::bail!("HTTP 状态码: {}", response.status());
            }
        }
        Err(e) => {
            anyhow::bail!("请求用户信息失败: {}", e);
        }
    }
}

