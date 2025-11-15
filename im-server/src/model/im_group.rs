use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImGroup {
    pub group_id: String,
    pub owner_id: String,
    pub group_type: i32,
    pub group_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mute: Option<i16>,
    pub apply_join_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_member_count: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub introduction: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sequence: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
    pub del_flag: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verifier: Option<i16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub member_count: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct ImGroupMember {
    pub group_member_id: String,
    pub group_id: String,
    pub member_id: String,
    pub role: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speak_date: Option<i64>,
    pub mute: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alias: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub leave_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<String>,
    pub del_flag: i16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub update_time: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i64>,
}

