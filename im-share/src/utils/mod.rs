use crate::model::ChatMessage;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn mqtt_user_topic(user_id: &str) -> String {
    format!("user/{user_id}/inbox")
}

pub fn encode_message(message: &ChatMessage) -> serde_json::Result<Vec<u8>> {
    serde_json::to_vec(message)
}

pub fn decode_message(bytes: &[u8]) -> serde_json::Result<ChatMessage> {
    serde_json::from_slice(bytes)
}

/// 获取当前时间戳（毫秒）
pub fn now_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}

/// 获取当前时间戳（秒）
pub fn now_timestamp_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}
