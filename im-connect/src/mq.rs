//! RabbitMQ 消费：按 brokerId 队列收消息，按 code 分发到 UserChannelMap

use crate::channel::UserChannelMap;
use crate::constants::code;
use crate::message::IMessageWrapJson;
use futures::StreamExt;
use lapin::options::*;
use lapin::Connection;
use std::sync::Arc;
use tracing::{debug, error, info, warn};

pub async fn run_consumer(
    amqp_url: &str,
    queue_name: &str,
    exchange: &str,
    error_routing_key: &str,
    channel_map: Arc<UserChannelMap>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 自动重连循环
    let mut retry_count = 0;
    const MAX_RETRY_DELAY_SECS: u64 = 60; // 最大重试延迟 60 秒
    
    loop {
        match run_consumer_once(amqp_url, queue_name, exchange, error_routing_key, channel_map.clone()).await {
            Ok(()) => {
                // 正常退出（不应该发生，可能是连接正常关闭）
                if retry_count > 0 {
                    info!("RabbitMQ 连接已恢复");
                    retry_count = 0; // 重置重试计数
                }
                warn!("RabbitMQ 消费者退出，5 秒后重连...");
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
            Err(e) => {
                retry_count += 1;
                let error_msg = e.to_string();
                
                // 首次连接失败和后续重连失败使用不同的日志级别
                if retry_count == 1 {
                    error!("RabbitMQ 连接失败: {}", error_msg);
                } else {
                    error!("RabbitMQ 重连失败（第 {} 次尝试）: {}", retry_count, error_msg);
                }
                
                // 计算重试延迟（指数退避，最大 60 秒）
                let delay_secs = std::cmp::min(
                    MAX_RETRY_DELAY_SECS,
                    2_u64.pow(std::cmp::min(retry_count - 1, 5)) // 最多 2^5 = 32 秒
                );
                
                warn!("{} 秒后尝试重连 RabbitMQ...", delay_secs);
                tokio::time::sleep(tokio::time::Duration::from_secs(delay_secs)).await;
            }
        }
    }
}

/// 执行一次消费者连接和消费循环
/// 返回 Ok(()) 表示正常退出，Err 表示连接错误需要重连
async fn run_consumer_once(
    amqp_url: &str,
    queue_name: &str,
    exchange: &str,
    error_routing_key: &str,
    channel_map: Arc<UserChannelMap>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // 验证端口配置：检查是否使用了 HTTP 管理端口而不是 AMQP 端口
    if let Some(after_scheme) = amqp_url.strip_prefix("amqp://") {
        if let Some(at_pos) = after_scheme.find('@') {
            let host_port_vhost = &after_scheme[at_pos+1..];
            if let Some(colon_pos) = host_port_vhost.find(':') {
                let port_str = &host_port_vhost[colon_pos+1..];
                if let Some(slash_pos) = port_str.find('/') {
                    let port = &port_str[..slash_pos];
                    if let Ok(port_num) = port.parse::<u16>() {
                        // 常见的 RabbitMQ HTTP 管理端口
                        if port_num == 15672 || port_num == 15671 {
                            let error_msg = format!(
                                "端口配置错误：端口 {} 是 RabbitMQ 管理界面的 HTTP 端口，不是 AMQP 协议端口！\n\
                                请使用 AMQP 端口 5672（或 5671 for TLS）进行连接。\n\
                                当前连接 URL: {}\n\
                                建议修改为: amqp://...@{}:5672/...",
                                port_num, amqp_url, host_port_vhost.split(':').next().unwrap_or("")
                            );
                            error!("{}", error_msg);
                            return Err(error_msg.into());
                        }
                    }
                }
            }
        }
    }
    
    // 尝试连接，添加更详细的错误处理
    info!("正在连接 RabbitMQ: {}", amqp_url);
    debug!("开始建立 RabbitMQ TCP 连接...");
    let conn = match Connection::connect(amqp_url, lapin::ConnectionProperties::default()).await {
        Ok(c) => {
            info!("✓ RabbitMQ TCP 连接已建立");
            c
        }
        Err(e) => {
            let error_msg = e.to_string();
            error!("RabbitMQ 连接失败: {}", error_msg);
            
            // 解析URL以提供更详细的诊断信息（手动解析，避免额外依赖）
            let mut diagnostic_info = String::new();
            // 格式: amqp://user:pass@host:port/vhost
            if let Some(after_scheme) = amqp_url.strip_prefix("amqp://") {
                if let Some(at_pos) = after_scheme.find('@') {
                    let user_pass = &after_scheme[..at_pos];
                    let host_port_vhost = &after_scheme[at_pos+1..];
                    if let Some(colon_pos) = user_pass.find(':') {
                        let username = &user_pass[..colon_pos];
                        diagnostic_info.push_str(&format!("\n连接详情:"));
                        diagnostic_info.push_str(&format!("\n  用户名: {}", username));
                    }
                    if let Some(colon_pos) = host_port_vhost.find(':') {
                        let host = &host_port_vhost[..colon_pos];
                        if let Some(slash_pos) = host_port_vhost[colon_pos+1..].find('/') {
                            let port = &host_port_vhost[colon_pos+1..colon_pos+1+slash_pos];
                            let vhost = &host_port_vhost[colon_pos+1+slash_pos..];
                            diagnostic_info.push_str(&format!("\n  主机: {}", host));
                            diagnostic_info.push_str(&format!("\n  端口: {}", port));
                            diagnostic_info.push_str(&format!("\n  虚拟主机: {}", vhost));
                        }
                    }
                }
            }
            
            // 检查是否是端口配置错误（使用了 HTTP 端口）
            let mut port_warning = String::new();
            if let Some(after_scheme) = amqp_url.strip_prefix("amqp://") {
                if let Some(at_pos) = after_scheme.find('@') {
                    let host_port_vhost = &after_scheme[at_pos+1..];
                    if let Some(colon_pos) = host_port_vhost.find(':') {
                        let port_str = &host_port_vhost[colon_pos+1..];
                        if let Some(slash_pos) = port_str.find('/') {
                            let port = &port_str[..slash_pos];
                            if let Ok(port_num) = port.parse::<u16>() {
                                if port_num == 15672 || port_num == 15671 {
                                    port_warning = format!(
                                        "\n\n⚠️  端口配置错误：端口 {} 是 RabbitMQ 管理界面的 HTTP 端口！\n\
                                        标准 AMQP 端口是 5672（或 5671 for TLS）。\n\
                                        请修改 --rabbitmq-port 参数为 5672",
                                        port_num
                                    );
                                }
                            }
                        }
                    }
                }
            }
            
            let mut detailed_error = format!("RabbitMQ 连接失败: {}{}{}", error_msg, diagnostic_info, port_warning);
            
            // 提供更详细的错误提示
            if error_msg.contains("ConnectionAborted") || error_msg.contains("connection aborted") {
                if port_warning.is_empty() {
                    detailed_error.push_str("\n\n可能的原因：");
                    detailed_error.push_str("\n  1. 端口配置错误：确认端口上运行的是 AMQP 协议（不是 HTTP）");
                    detailed_error.push_str("\n  2. 认证失败：检查用户名和密码是否正确");
                    detailed_error.push_str("\n  3. 虚拟主机不存在或用户无权限访问");
                    detailed_error.push_str("\n  4. RabbitMQ 服务配置问题：端口可能未正确配置为 AMQP 监听");
                    detailed_error.push_str("\n  5. 防火墙或网络问题");
                    detailed_error.push_str("\n\n建议：");
                    detailed_error.push_str("\n  - 使用 telnet 或 nc 测试端口连通性");
                    detailed_error.push_str("\n  - 检查 RabbitMQ 日志确认连接请求");
                    detailed_error.push_str("\n  - 确认端口配置（标准 AMQP 端口是 5672）");
                }
            } else if error_msg.contains("refused") {
                detailed_error.push_str("\n\n可能的原因：");
                detailed_error.push_str("\n  1. RabbitMQ 服务未启动");
                detailed_error.push_str("\n  2. 端口配置错误");
                detailed_error.push_str("\n  3. 防火墙阻止了连接");
            } else if error_msg.contains("timeout") {
                detailed_error.push_str("\n\n可能的原因：");
                detailed_error.push_str("\n  1. 网络延迟过高");
                detailed_error.push_str("\n  2. 防火墙阻止了连接");
                detailed_error.push_str("\n  3. RabbitMQ 服务响应慢");
            }
            
            return Err(detailed_error.into());
        }
    };
    
    let channel = conn.create_channel().await?;
    
    // 声明 exchange（如果不存在则创建，持久化）
    info!("声明 RabbitMQ exchange: {} (类型: direct, 持久化)", exchange);
    channel.exchange_declare(
        exchange,
        lapin::ExchangeKind::Direct,
        ExchangeDeclareOptions {
            passive: false,  // 如果不存在则创建
            durable: true,   // 持久化
            auto_delete: false,
            internal: false,
            nowait: false,
        },
        lapin::types::FieldTable::default(),
    ).await?;
    
    // 声明队列（如果不存在则创建）
    // 直接尝试创建独占队列，如果队列已存在且被其他连接独占，会返回 RESOURCE_LOCKED 错误
    info!("声明 RabbitMQ 队列: {} (独占、自动删除)", queue_name);
    match channel.queue_declare(
        queue_name,
        QueueDeclareOptions {
            passive: false,  // 创建新队列
            durable: false,  // 非持久化（exclusive 队列通常不需要持久化）
            exclusive: true, // 独占队列
            auto_delete: true, // 自动删除
            nowait: false,
        },
        lapin::types::FieldTable::default(),
    ).await {
        Ok(_) => {
            info!("队列 {} 创建成功", queue_name);
        }
        Err(e) => {
            let error_msg = e.to_string();
            if error_msg.contains("RESOURCE_LOCKED") || error_msg.contains("405") {
                // 队列已被其他连接独占使用
                return Err(format!(
                    "队列 {} 已被其他连接独占使用。请确保没有其他 im-connect 实例使用相同的 broker_id ({})，或等待其他实例关闭后重试",
                    queue_name, queue_name
                ).into());
            } else if error_msg.contains("NOT_FOUND") || error_msg.contains("404") {
                // 这种情况不应该发生，因为 passive=false 应该会创建队列
                return Err(format!(
                    "队列 {} 创建失败: {}",
                    queue_name, error_msg
                ).into());
            } else {
                // 其他错误
                return Err(format!(
                    "队列 {} 声明失败: {}",
                    queue_name, error_msg
                ).into());
            }
        }
    }
    
    // 绑定队列到 exchange
    info!("绑定队列 {} 到 exchange {} (routing_key: {})", queue_name, exchange, queue_name);
    channel.queue_bind(
        queue_name,
        exchange,
        queue_name,
        QueueBindOptions::default(),
        lapin::types::FieldTable::default(),
    ).await?;
    
    // 声明错误队列（如果不存在则创建，持久化）
    info!("声明 RabbitMQ 错误队列: {}", error_routing_key);
    channel.queue_declare(
        error_routing_key,
        QueueDeclareOptions {
            passive: false,
            durable: true,   // 错误队列需要持久化
            exclusive: false,
            auto_delete: false,
            nowait: false,
        },
        lapin::types::FieldTable::default(),
    ).await?;
    
    // 绑定错误队列到 exchange
    channel.queue_bind(
        error_routing_key,
        exchange,
        error_routing_key,
        QueueBindOptions::default(),
        lapin::types::FieldTable::default(),
    ).await?;

    let mut consumer = channel
        .basic_consume(
            queue_name,
            "im-connect-consumer",
            BasicConsumeOptions::default(),
            lapin::types::FieldTable::default(),
        )
        .await?;

    info!("RabbitMQ 消费已启动: queue={}", queue_name);

    while let Some(delivery) = consumer.next().await {
        let delivery = match delivery {
            Ok(d) => d,
            Err(e) => {
                let error_msg = e.to_string();
                error!("消费接收错误: {}", error_msg);
                
                // 如果是连接错误或超时错误，退出循环以触发重连
                if error_msg.contains("timeout") 
                    || error_msg.contains("Connection error")
                    || error_msg.contains("connection aborted")
                    || error_msg.contains("broken")
                    || error_msg.contains("IO error") {
                    return Err(format!("RabbitMQ 连接错误，需要重连: {}", error_msg).into());
                }
                continue;
            }
        };
        let body = String::from_utf8_lossy(&delivery.data);
        match handle_message(&body, &channel_map) {
            Ok(()) => {
                if delivery.ack(BasicAckOptions::default()).await.is_err() {
                    error!("ack 失败");
                }
            }
            Err(e) => {
                error!("处理消息失败: {} body={}", e, body);
                let _ = channel
                    .basic_publish(
                        exchange,
                        error_routing_key,
                        BasicPublishOptions::default(),
                        body.as_bytes(),
                        lapin::BasicProperties::default(),
                    )
                    .await;
                let _ = delivery.nack(BasicNackOptions { requeue: false, multiple: false }).await;
            }
        }
    }
    Ok(())
}

fn handle_message(
    body: &str,
    channel_map: &UserChannelMap,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let wrap: IMessageWrapJson = serde_json::from_str(body)?;
    let code = wrap.code.unwrap_or(0);
    let ids = wrap.ids.clone().unwrap_or_default();
    let proto_bytes = json_wrap_to_proto_bytes(&wrap)?;

    match code {
        code::SINGLE_MESSAGE | code::GROUP_MESSAGE | code::VIDEO_MESSAGE | code::GROUP_OPERATION
        | code::MESSAGE_OPERATION => {
            if ids.is_empty() {
                debug!("消息目标 ids 为空，忽略");
                return Ok(());
            }
            for user_id in ids {
                let txs = channel_map.get_all_tx_by_user(&user_id);
                for tx in txs {
                    if tx.send(proto_bytes.clone()).is_err() {
                        debug!("用户 {} 通道已关闭，跳过", user_id);
                    }
                }
            }
        }
        code::FORCE_LOGOUT => {
            for user_id in &ids {
                let txs = channel_map.get_all_tx_by_user(user_id);
                for tx in txs {
                    let _ = tx.send(proto_bytes.clone());
                }
            }
        }
        _ => {
            debug!("未处理消息类型: code={}", code);
        }
    }
    Ok(())
}

fn json_wrap_to_proto_bytes(wrap: &IMessageWrapJson) -> Result<Vec<u8>, Box<dyn std::error::Error + Send + Sync>> {
    use crate::proto::im::connect::IMessageWrap;
    use prost::Message;
    let data = wrap.data.as_ref().map(|v| {
        let bytes = serde_json::to_vec(v).unwrap_or_default();
        crate::proto::Any {
            type_url: "json".to_string(),
            value: bytes,
        }
    });
    let msg = IMessageWrap {
        code: wrap.code.unwrap_or(0),
        token: wrap.token.clone().unwrap_or_default(),
        data,
        metadata: wrap.metadata.clone().unwrap_or_default(),
        message: wrap.message.clone().unwrap_or_default(),
        request_id: wrap.request_id.clone().unwrap_or_default(),
        timestamp: wrap.timestamp.unwrap_or(0),
        client_ip: wrap.client_ip.clone().unwrap_or_default(),
        user_agent: wrap.user_agent.clone().unwrap_or_default(),
        device_name: wrap.device_name.clone().unwrap_or_default(),
        device_type: wrap.device_type.clone().unwrap_or_default(),
    };
    let mut buf = Vec::with_capacity(msg.encoded_len());
    msg.encode(&mut buf)?;
    Ok(buf)
}
