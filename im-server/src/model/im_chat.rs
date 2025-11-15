use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImChat {
    pub chat_id: String,
    pub chat_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub to_id: String,
    pub is_mute: i16,
    pub is_top: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remark: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub del_flag: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

/// 聊天信息，包含关联的名称信息（群组名称或用户名）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatWithName {
    pub chat_id: String,
    pub chat_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_id: Option<String>,
    pub to_id: String,
    pub is_mute: i16,
    pub is_top: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub read_sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub del_flag: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    /// 名称（群组名称或用户名）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// 群组人数（仅群组有效）
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i32>,
}

