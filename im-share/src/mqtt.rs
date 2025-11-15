use anyhow::Result;
use rumqttc::{AsyncClient, Event, EventLoop, MqttOptions, Packet, QoS};
use std::time::Duration;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

#[derive(Clone, Debug)]
pub struct MqttConfig {
    pub host: String,
    pub port: u16,
    pub client_id: String,
    pub keep_alive_secs: u64,
}

impl MqttConfig {
    pub fn new(host: impl Into<String>, port: u16, client_id: impl Into<String>) -> Self {
        Self { host: host.into(), port, client_id: client_id.into(), keep_alive_secs: 30 }
    }
}

#[derive(Clone, Debug)]
pub struct IncomingMessage {
    pub topic: String,
    pub payload: Vec<u8>,
}

#[derive(Clone)]
pub struct ImMqtt {
    client: AsyncClient,
    tx: broadcast::Sender<IncomingMessage>,
}

impl ImMqtt {
    pub fn connect(config: MqttConfig) -> Self {
        let mut options = MqttOptions::new(config.client_id, config.host, config.port);
        options.set_keep_alive(Duration::from_secs(config.keep_alive_secs));
        // 设置 clean_session = false，让 broker 为离线客户端存储消息（QoS 1 或 2）
        // 这样即使客户端突然断线，broker 也能在重连后推送离线消息
        options.set_clean_session(false);
        let (client, eventloop): (AsyncClient, EventLoop) = AsyncClient::new(options, 10);
        let (tx, _rx) = broadcast::channel(256);
        let tx_clone = tx.clone();
        tokio::spawn(async move {
            let mut ev = eventloop;
            let mut last_conn_ack_time = std::time::Instant::now();
            let mut is_connected = false;
            
            loop {
                match ev.poll().await {
                    Ok(event) => {
                        match event {
                            Event::Incoming(Packet::Publish(p)) => {
                                // 在移动之前先克隆 topic，用于后续日志
                                let topic = p.topic.clone();
                                let payload_len = p.payload.len();
                                let qos = p.qos;
                                
                                // 尝试解析消息内容用于调试
                                // 注意：p.payload 是 Bytes 类型，需要转换为 Vec<u8> 或使用 as_ref()
                                let message_info = if let Ok(text) = String::from_utf8(p.payload.to_vec()) {
                                    if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                                        format!("message_id={:?}, chat_type={:?}, from={:?}, to={:?}", 
                                            json.get("message_id"),
                                            json.get("chat_type"),
                                            json.get("from_user_id"),
                                            json.get("to_user_id"))
                                    } else {
                                        format!("payload_preview={}", text.chars().take(50).collect::<String>())
                                    }
                                } else {
                                    "binary_payload".to_string()
                                };
                                
                                // 检查是否有订阅者（在记录日志之前检查，以便在日志中包含这个信息）
                                let receiver_count = tx_clone.receiver_count();
                                
                                info!(
                                    topic = %topic, 
                                    payload_len = payload_len, 
                                    qos = ?qos,
                                    message_info = %message_info,
                                    receiver_count = receiver_count,
                                    "✅ 收到MQTT消息（从broker推送，QoS {:?}，当前有 {} 个订阅者）",
                                    qos,
                                    receiver_count
                                );
                                
                                if receiver_count == 0 {
                                    // 没有订阅者（WebSocket 可能已断开），这是正常情况
                                    // 但使用 warn 级别，因为这可能表示问题
                                    warn!(
                                        topic = %topic,
                                        payload_len = payload_len,
                                        message_info = %message_info,
                                        "⚠️ MQTT消息无订阅者（WebSocket可能已断开或尚未订阅），消息将被丢弃"
                                    );
                                } else {
                                    // 有订阅者，尝试发送
                                    match tx_clone.send(IncomingMessage { topic: p.topic, payload: p.payload.to_vec() }) {
                                        Ok(actual_receivers) => {
                                            info!(
                                                topic = %topic,
                                                receiver_count = receiver_count,
                                                actual_receivers = actual_receivers,
                                                "✅ 消息已发送到broadcast channel（{} 个接收者）",
                                                actual_receivers
                                            );
                                        },
                                        Err(e) => {
                                            // 发送失败（可能是 channel 已关闭），使用 warn 级别
                                            warn!(
                                                topic = %topic,
                                                error = %e,
                                                receiver_count = receiver_count,
                                                "❌ 发送消息到broadcast channel失败（channel可能已关闭）"
                                            );
                                        }
                                    }
                                }
                            }
                            Event::Incoming(Packet::ConnAck(ack)) => {
                                let now = std::time::Instant::now();
                                let time_since_last_conn = now.duration_since(last_conn_ack_time);
                                last_conn_ack_time = now;
                                
                                // 只有在连接状态改变或距离上次连接确认超过5秒时才记录日志
                                if !is_connected || time_since_last_conn.as_secs() > 5 {
                                    info!(session_present = ack.session_present, "MQTT 连接已建立，会话状态: {}", if ack.session_present { "已恢复" } else { "新会话" });
                                }
                                is_connected = true;
                            }
                            Event::Incoming(Packet::SubAck(sa)) => {
                                info!(packet_id = sa.pkid, "✅ MQTT 订阅确认（broker将推送离线消息，QoS 1 + clean_session=false）");
                                // 订阅确认后，broker应该会立即推送离线消息（如果有的话）
                                // 使用 QoS 1 和 clean_session=false，broker会为已订阅的客户端存储消息
                            }
                            Event::Incoming(Packet::Disconnect) => {
                                debug!("收到 MQTT Disconnect 包");
                                is_connected = false;
                            }
                            Event::Outgoing(_) => {
                                // 忽略出站事件
                            }
                            _ => {
                                // 其他事件可以在这里处理
                            }
                        }
                    }
                    Err(e) => {
                        let error_str = e.to_string();
                        // 检查是否是连接关闭相关的错误（这些是正常的网络断开情况）
                        let is_connection_closed = error_str.contains("Connection closed by peer")
                            || error_str.contains("connection closed by peer")
                            || error_str.contains("Connection closed")
                            || error_str.contains("connection closed")
                            || error_str.contains("Connection reset")
                            || error_str.contains("connection reset")
                            || error_str.contains("Broken pipe")
                            || error_str.contains("broken pipe");
                        
                        // 注意：不需要设置 is_connected = false，因为后面会直接 break
                        
                        if is_connection_closed {
                            // 连接关闭，直接退出循环，不再重连
                            warn!(
                                error = %e,
                                "MQTT 连接已关闭，退出事件循环"
                            );
                            break;
                        } else {
                            // 其他错误，记录日志后退出
                            error!(
                                error = %e,
                                "MQTT EventLoop 错误，退出事件循环"
                            );
                            break;
                        }
                    }
                }
            }
        });
        Self { client, tx }
    }

    pub async fn publish(&self, topic: &str, payload: Vec<u8>) -> Result<()> {
        // 使用 QoS::AtLeastOnce (QoS 1) 确保消息至少被传递一次
        // retain=false 表示不保留消息（retain消息会一直保留在broker上，直到被新的retain消息覆盖）
        // 对于离线消息，QoS 1 + clean_session=false 已经足够，broker会为已订阅的客户端存储消息
        // 
        // 重要：broker只会为已经订阅过的客户端存储离线消息
        // 如果消息在订阅之前发布，broker不会存储（因为客户端还没有订阅）
        // 所以消息必须保存到数据库，作为离线消息的备份
        let payload_len = payload.len();
        
        // 尝试解析 payload 以获取消息信息用于日志
        let message_info = if let Ok(text) = String::from_utf8(payload.clone()) {
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                format!("message_id={:?}, from_user_id={:?}, to_user_id={:?}, chat_type={:?}", 
                    json.get("message_id"),
                    json.get("from_user_id"),
                    json.get("to_user_id"),
                    json.get("chat_type"))
            } else {
                format!("payload_preview={}", text.chars().take(100).collect::<String>())
            }
        } else {
            "binary_payload".to_string()
        };
        
        match self.client.publish(topic, QoS::AtLeastOnce, false, payload).await {
            Ok(_) => {
                info!(
                    topic = %topic, 
                    payload_len = payload_len, 
                    qos = "QoS 1",
                    message_info = %message_info,
                    "✅ MQTT消息已发布（QoS 1，retain=false）"
                );
                Ok(())
            },
            Err(e) => {
                error!(
                    topic = %topic,
                    payload_len = payload_len,
                    message_info = %message_info,
                    error = %e,
                    "❌ MQTT消息发布失败"
                );
                Err(e.into())
            }
        }
    }

    pub async fn subscribe(&self, topic: &str) -> Result<broadcast::Receiver<IncomingMessage>> {
        // 使用 QoS::AtLeastOnce (QoS 1) 确保订阅至少被确认一次
        // 配合 clean_session=false，broker会为这个订阅存储离线消息
        // 
        // 重要说明：
        // 1. broker只会为已经订阅过的客户端存储消息（QoS 1或2）
        // 2. 如果消息在订阅之前发布，broker不会存储（因为客户端还没有订阅）
        // 3. 订阅确认后，broker会立即推送离线消息（如果有的话）
        // 4. 使用固定的client_id（基于用户ID）确保会话可以恢复
        self.client.subscribe(topic, QoS::AtLeastOnce).await?;
        info!(topic = %topic, qos = "QoS 1", "MQTT订阅已发送（QoS 1），等待broker确认和推送离线消息");
        // 返回接收者，注意：这里返回的是新的接收者，不会收到订阅之前发布的消息
        // 但是broker会在订阅确认后推送离线消息（如果客户端之前订阅过且clean_session=false）
        Ok(self.tx.subscribe())
    }

    pub async fn unsubscribe(&self, topic: &str) -> Result<()> {
        self.client.unsubscribe(topic).await?;
        Ok(())
    }

    /// 断开 MQTT 连接
    /// 注意：断开连接不会清除订阅信息（因为 clean_session=false）
    /// 只有取消订阅（unsubscribe）才会清除订阅信息
    pub async fn disconnect(&self) -> Result<()> {
        // rumqttc 的 AsyncClient 在 drop 时会自动断开连接
        // 但我们可以通过发送一个断开信号来确保连接及时断开
        // 实际上，rumqttc 没有显式的 disconnect 方法
        // 当 AsyncClient 被 drop 时，连接会自动断开
        // 这里我们只是记录日志，实际的断开会在 drop 时发生
        info!("MQTT 客户端断开请求（连接将在 drop 时断开）");
        Ok(())
    }
}


