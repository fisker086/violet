use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct IdMetaInfo {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<i32>,
    pub update_time: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<i32>,
}

