use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub message_id: String,
    pub from_user_id: String,
    pub to_user_id: String,
    pub message: String,
    pub timestamp_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
    /// 聊天类型：1=单聊，2=群聊
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chat_type: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "to_type", content = "to_id")]
pub enum Target {
    User(String),
    Group(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SendRequest {
    pub from_user_id: String,
    pub target: Target,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
}

