use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImSingleMessage {
    pub message_id: String,
    pub from_id: String,
    pub to_id: String,
    pub message_body: String,
    pub message_time: i64,
    pub message_content_type: i32,
    pub read_status: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
    pub del_flag: i16,
    pub sequence: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_random: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImGroupMessage {
    pub message_id: String,
    pub group_id: String,
    pub from_id: String,
    pub message_body: String,
    pub message_time: i64,
    pub message_content_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
    pub del_flag: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_random: Option<String>,
    pub create_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply_to: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImGroupMessageStatus {
    pub group_id: String,
    pub message_id: String,
    pub to_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}


#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImOutbox {
    pub id: u64,
    pub message_id: String,
    pub payload: String,
    pub exchange: String,
    pub routing_key: String,
    pub attempts: i32,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_try_at: Option<i64>,
}

