//! 消息封装：Proto 编解码、RabbitMQ JSON 结构、按 code 分发

use prost::Message;
use serde::Deserialize;
use std::collections::HashMap;

/// RabbitMQ 下发的 JSON 与内存中的 IMessageWrap（含 ids 等服务端字段）
#[derive(Debug, Clone, Deserialize)]
pub struct IMessageWrapJson {
    pub code: Option<i32>,
    pub token: Option<String>,
    pub data: Option<serde_json::Value>,
    pub ids: Option<Vec<String>>,
    pub metadata: Option<HashMap<String, String>>,
    pub message: Option<String>,
    pub request_id: Option<String>,
    pub timestamp: Option<i64>,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub device_name: Option<String>,
    pub device_type: Option<String>,
}

/// 构造仅含 code/message 的 proto 并序列化为 bytes（用于踢人等）
pub fn wrap_to_proto_bytes(
    code: i32,
    token: Option<&str>,
    message: Option<&str>,
    metadata: Option<HashMap<String, String>>,
) -> Vec<u8> {
    use crate::proto::im::connect::IMessageWrap;
    let msg = IMessageWrap {
        code,
        token: token.map(String::from).unwrap_or_default(),
        data: None,
        metadata: metadata.unwrap_or_default(),
        message: message.unwrap_or("").to_string(),
        request_id: uuid::Uuid::new_v4().to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        client_ip: String::new(),
        user_agent: String::new(),
        device_name: String::new(),
        device_type: String::new(),
    };
    msg.encode_to_vec()
}

/// 从二进制解析 IMessageWrap（WebSocket 上行）
pub fn decode_wrap_from_bytes(buf: &[u8]) -> Result<crate::proto::im::connect::IMessageWrap, prost::DecodeError> {
    crate::proto::im::connect::IMessageWrap::decode(buf)
}

/// 将 proto 转为 bytes（WebSocket 下行）
#[allow(dead_code)]
pub fn encode_wrap_to_bytes(msg: &crate::proto::im::connect::IMessageWrap) -> Vec<u8> {
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf).expect("encode");
    buf
}
