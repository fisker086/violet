use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct User {
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub open_id: Option<String>,
    pub name: String,
    pub email: String,
    #[serde(skip_serializing)]
    #[sqlx(default)]
    pub password_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[serde(rename(serialize = "abstract"))] // 序列化时使用 abstract，反序列化时仍接受 abstract_field
    #[sqlx(default)]
    pub abstract_field: Option<String>, // abstract 是 Rust 关键字，使用 abstract_field
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub phone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub status: Option<i8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[sqlx(default)]
    pub gender: Option<i8>,
}

impl User {
    pub fn new(id: u64, open_id: Option<String>, name: String, email: String) -> Self {
        User {
            id,
            open_id,
            name,
            email,
            password_hash: None,
            file_name: None,
            abstract_field: None,
            phone: None,
            status: Some(1), // 默认状态：正常
            gender: Some(3), // 默认性别：未知
        }
    }
    
    /// 获取用于MQTT的ID（从open_id解析，如果是数字字符串则解析，否则使用id）
    /// 内部服务使用，不对外暴露
    pub fn get_mqtt_id(&self) -> u64 {
        // 如果 open_id 是数字字符串（雪花算法生成的），解析它
        if let Some(ref open_id) = self.open_id {
            if let Ok(id) = open_id.parse::<u64>() {
                return id;
            }
        }
        // 否则使用数据库 id
        self.id
    }
    
    /// 获取外部标识符（只使用 open_id）
    /// 用于聊天记录等场景，确保使用稳定的唯一标识符
    /// 注意：所有用户都应该有 open_id（在登录和认证中间件中确保）
    pub fn get_external_id(&self) -> String {
        // 只使用 open_id，不再使用用户名、手机号或数据库ID
        // 如果 open_id 不存在或为空，返回错误（这种情况不应该发生）
        if let Some(ref open_id) = self.open_id {
            if !open_id.is_empty() {
                return open_id.clone();
            }
        }
        // 如果没有 open_id，这是一个错误情况（应该在认证中间件中已经确保）
        // 为了向后兼容，暂时使用 id，但应该尽快修复数据
        tracing::warn!(
            user_id = self.id,
            user_name = %self.name,
            "用户没有 open_id，使用数据库 id 作为后备（应该尽快修复）"
        );
        self.id.to_string()
    }
    
    /// 从 open_id 解析数字ID（用于MQTT、JWT等需要数字的场景）
    /// 如果 open_id 是数字字符串则解析，否则返回 None
    #[allow(dead_code)]
    pub fn parse_open_id_as_number(&self) -> Option<u64> {
        self.open_id.as_ref()
            .and_then(|oid| oid.parse::<u64>().ok())
    }
    
    /// 检查用户状态是否正常
    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.status.unwrap_or(1) == 1
    }
}