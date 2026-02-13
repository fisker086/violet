//! 用户登录/注册逻辑：JWT 验证、Redis 会话存储

use crate::auth;
use crate::constants;
use crate::device::IMDeviceType;
use crate::message;
use crate::proto::im::connect::IMessageWrap;
use redis::cmd;
use serde_json::json;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

/// 处理用户注册/登录请求
pub async fn handle_register(
    wrap: &IMessageWrap,
    state: &crate::ws::AppState,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 1. 提取 token
    let token = wrap.token.trim();
    if token.is_empty() {
        let error_msg = message::wrap_to_proto_bytes(
            constants::code::ERROR,
            None,
            Some("Token 不能为空"),
            None,
        );
        tx.send(error_msg)?;
        return Ok(());
    }

    // 2. 验证 JWT token
    let user_id = match auth::validate_token(&state.jwt_secret, token) {
        Ok(uid) => uid,
        Err(e) => {
            warn!("Token 验证失败: {}", e);
            let error_msg = message::wrap_to_proto_bytes(
                constants::code::ERROR,
                None,
                Some(&format!("Token 验证失败: {}", e)),
                None,
            );
            tx.send(error_msg)?;
            return Ok(());
        }
    };

    // 3. 提取设备类型
    let device_type_str = wrap.device_type.trim();
    let device_type = if device_type_str.is_empty() {
        IMDeviceType::WEB
    } else {
        IMDeviceType::of_or_default(device_type_str, IMDeviceType::WEB)
    };

    // 4. 将用户会话存储到 Redis
    // Redis Key: IM-USER-{userId}
    // Value: JSON 格式，包含 broker_id, device_type, channel_id 等信息
    let redis_key = format!("IM-USER-{}", user_id);
    let channel_id = uuid::Uuid::new_v4().to_string();
    
    let session_data = json!({
        "broker_id": state.broker_id,
        "device_type": device_type.type_name,
        "device_group": format!("{:?}", device_type.group),
        "channel_id": channel_id,
        "login_time": chrono::Utc::now().timestamp_millis(),
    });

    // 存储到 Redis，设置过期时间（使用超时时间）
    let ttl_seconds = (state.timeout_ms / 1000) as i64;
    let redis_key_clone = redis_key.clone();
    let session_data_str = session_data.to_string();
    // MultiplexedConnection 是 Clone 的，可以直接克隆使用
    let mut redis_conn = (*state.redis).clone();
    
    tokio::spawn(async move {
        // 使用 redis::cmd 执行 SETEX 命令（一次性设置值和过期时间）
        // MultiplexedConnection 实现了 ConnectionLike，exec_async 需要 &mut self
        match cmd("SETEX")
            .arg(&redis_key_clone)
            .arg(ttl_seconds)
            .arg(&session_data_str)
            .exec_async(&mut redis_conn)
            .await
        {
            Ok(_) => {
                info!("用户会话已存储到 Redis: key={}, ttl={}s", redis_key_clone, ttl_seconds);
            }
            Err(e) => {
                error!("Redis 存储用户会话失败: key={}, error={}", redis_key_clone, e);
            }
        }
    });

    // 5. 将连接添加到 channel_map
    state.channel_map.add_channel(user_id.clone(), device_type.clone(), tx.clone());

    info!(
        "用户登录成功: userId={}, deviceType={}, brokerId={}, channelId={}",
        user_id, device_type.type_name, state.broker_id, channel_id
    );

    // 6. 返回注册成功消息
    let success_msg = message::wrap_to_proto_bytes(
        constants::code::REGISTER_SUCCESS,
        Some(token),
        Some("登录成功"),
        None,
    );
    tx.send(success_msg)?;

    Ok(())
}
