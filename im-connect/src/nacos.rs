//! Nacos 服务注册：将 WebSocket 端口注册到 Nacos

use crate::config::NacosConfig;
use nacos_sdk::api::naming::{NamingService, NamingServiceBuilder, ServiceInstance};
use nacos_sdk::api::props::ClientProps;
use std::collections::HashMap;
use std::time::Duration;
use tracing::{debug, error, info, warn};

pub async fn register_websocket_ports(
    cfg: &NacosConfig,
    ports: &[u16],
    local_ip: &str,
    broker_id: &str,
    ws_path: &str,
    protocol: &str,
    region: Option<&str>,
    priority: Option<i32>,
    version: Option<&str>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !cfg.register_enabled || ports.is_empty() {
        return Ok(());
    }

    info!("开始注册 WebSocket 端口到 Nacos");
    info!("  - 服务名: {}", cfg.service_name);
    info!("  - 分组: {}", cfg.group);
    info!("  - 服务器地址: {}", cfg.server_addr);
    info!("  - 本机 IP: {}", local_ip);
    info!("  - 端口列表: {:?}", ports);
    if let Some(ref ns) = cfg.namespace {
        if !ns.is_empty() {
            info!("  - 命名空间: {}", ns);
        }
    }
    if cfg.username.is_some() && cfg.password.is_some() {
        info!("  - 认证: 已启用");
    }

    // 构建 ClientProps
    debug!("构建 Nacos ClientProps...");
    let mut props = ClientProps::new()
        .server_addr(cfg.server_addr.clone())
        .app_name(cfg.service_name.clone());
    
    // 设置命名空间
    if let Some(ref ns) = cfg.namespace {
        if !ns.is_empty() {
            props = props.namespace(ns.clone());
            debug!("设置命名空间: {}", ns);
        }
    }
    
    // 设置认证信息（如果提供）
    if let (Some(ref username), Some(ref password)) = (&cfg.username, &cfg.password) {
        props = props.auth_username(username.clone()).auth_password(password.clone());
        debug!("设置认证信息: username={}", username);
    }

    // 创建 NamingService
    info!("正在连接 Nacos 服务器: {}...", cfg.server_addr);
    
    // 检查端口配置提示
    let host = cfg.server_addr.split(':').next().unwrap_or("127.0.0.1");
    if let Some(port_str) = cfg.server_addr.split(':').nth(1) {
        if let Ok(port) = port_str.parse::<u16>() {
            if port == 8080 {
                warn!("⚠️  警告: 端口 8080 通常不是 Nacos 的标准端口");
                warn!("   Nacos 标准端口: 8848 (HTTP API) 或 9848 (gRPC)");
                warn!("   当前配置: {}", cfg.server_addr);
                warn!("   SDK 会尝试连接 gRPC 端口: {} (8080 + 1000)", port + 1000);
                warn!("   但 Nacos gRPC 标准端口是 9848");
                warn!("");
                warn!("   解决方案：");
                warn!("   1. 如果 Nacos HTTP 端口可访问，请使用: {}:8848", host);
                warn!("   2. 如果必须使用反向代理 (8080)，请确保代理转发 gRPC 流量到端口 9848");
                warn!("   3. 测试 gRPC 端口: telnet {} 9848", host);
            } else if port != 8848 && port != 9848 {
                info!("提示: Nacos SDK 使用 gRPC 连接");
                info!("   SDK 会自动尝试端口 {} (HTTP) 和 {} (gRPC)", port, port + 1000);
                info!("   如果连接失败，请确认 Nacos gRPC 端口 {} 可访问", port + 1000);
                info!("   标准 Nacos gRPC 端口是 9848");
            }
        }
    }
    
    // 尝试创建 NamingService（带重试）
    let naming_service = match create_naming_service_with_retry(cfg, 3).await {
        Ok(service) => {
            info!("✓ Nacos NamingService 创建成功");
            service
        }
        Err(e) => {
            let error_msg = e.to_string();
            error!("✗ 创建 Nacos NamingService 失败: {}", error_msg);
            
            // 提供详细的错误诊断
            error!("\n=== Nacos 连接失败诊断 ===");
            error!("服务器地址: {}", cfg.server_addr);
            
            if error_msg.contains("Unavailable") || error_msg.contains("tcp connect error") || error_msg.contains("can't open server stream") {
                error!("\n可能的原因：");
                error!("  1. 端口配置错误：");
                error!("     - Nacos HTTP 端口通常是 8848");
                error!("     - Nacos gRPC 端口通常是 9848 (HTTP端口 + 1000)");
                error!("     - 当前配置: {}", cfg.server_addr);
                error!("     - SDK 会尝试连接 gRPC 端口: {}", 
                    cfg.server_addr.split(':').nth(1)
                        .and_then(|p| p.parse::<u16>().ok())
                        .map(|p| if p == 8848 { 9848 } else { p + 1000 })
                        .unwrap_or(9848));
                error!("  2. Nacos 服务未启动或无法访问");
                error!("  3. 防火墙或网络问题（检查 gRPC 端口是否开放）");
                error!("  4. 如果使用反向代理，请确认代理正确转发 gRPC 流量到端口 9848");
                error!("\n建议：");
                error!("  - 使用标准端口: {}:8848", host);
                error!("  - 测试 HTTP 连接: curl http://{}/nacos/v1/ns/operator/metrics", cfg.server_addr.split(':').next().unwrap_or("127.0.0.1"));
                error!("  - 测试 gRPC 端口: telnet {} {}", host, 
                    cfg.server_addr.split(':').nth(1)
                        .and_then(|p| p.parse::<u16>().ok())
                        .map(|p| if p == 8848 { 9848 } else { p + 1000 })
                        .unwrap_or(9848));
                error!("  - 检查 Nacos 日志确认 gRPC 服务是否正常启动");
            } else if error_msg.contains("Unauthorized") || error_msg.contains("authentication") {
                error!("\n可能的原因：");
                error!("  1. 用户名或密码错误");
                error!("  2. Nacos 认证未正确配置");
                error!("\n建议：");
                error!("  - 检查 --nacos-username 和 --nacos-password 参数");
                error!("  - 确认 Nacos 控制台中的用户名和密码");
            } else {
                error!("\n请检查:");
                error!("  1. Nacos 服务器地址是否正确: {}", cfg.server_addr);
                error!("  2. Nacos 服务是否已启动");
                error!("  3. 网络连接是否正常");
                if cfg.username.is_some() || cfg.password.is_some() {
                    error!("  4. 用户名和密码是否正确");
                }
            }
            
            return Err(format!("创建 Nacos NamingService 失败: {}", error_msg).into());
        }
    };

    // 为每个端口注册服务实例
    let mut registered_count = 0;
    for &port in ports {
        let instance_id = format!("{}-{}-{}", cfg.service_name, local_ip, port);
        
        // 构建服务实例元数据（参考老版本格式）
        let mut metadata = HashMap::new();
        // 基础信息
        metadata.insert("port".to_string(), port.to_string());
        metadata.insert("protocol".to_string(), "websocket".to_string());
        
        // brokerId: 使用传入的 broker_id
        metadata.insert("brokerId".to_string(), broker_id.to_string());
        
        // wsPath: WebSocket 路径
        metadata.insert("wsPath".to_string(), ws_path.to_string());
        
        // connection: 当前连接数，初始为 0
        metadata.insert("connection".to_string(), "0".to_string());
        
        // protocols: 协议列表，格式化为 JSON 数组字符串
        let protocols_json = format!(r#"["{}"]"#, protocol);
        metadata.insert("protocols".to_string(), protocols_json);
        
        // region: 区域信息（如果提供）
        if let Some(region) = region {
            metadata.insert("region".to_string(), region.to_string());
        }
        
        // priority: 优先级（默认: 1）
        let priority_value = priority.unwrap_or(1);
        metadata.insert("priority".to_string(), priority_value.to_string());
        
        // version: 版本号（默认: 1.0.0）
        let version_value = version.unwrap_or("1.0.0");
        metadata.insert("version".to_string(), version_value.to_string());
        
        // 保留原有的 broker_id 字段（兼容性）
        metadata.insert("broker_id".to_string(), format!("{}-{}", local_ip, port));

        // 创建服务实例
        // cluster_name 必须符合 Nacos 规范：只能包含 0-9a-zA-Z-. 字符
        // 将 group 中的下划线替换为连字符，或使用默认值 "DEFAULT"
        let cluster_name = sanitize_cluster_name(&cfg.group);
        if cluster_name != cfg.group {
            debug!("集群名称已清理: group={} -> cluster_name={}", cfg.group, cluster_name);
        }
        
        let instance = ServiceInstance {
            instance_id: Some(instance_id.clone()),
            ip: local_ip.to_string(),
            port: port as i32,
            weight: 1.0,
            healthy: true,
            enabled: true,
            ephemeral: true,
            cluster_name: Some(cluster_name),
            service_name: Some(cfg.service_name.clone()),
            metadata,
        };

        // 注册服务实例（使用 register_instance 方法，需要传入 service_name, group, instance）
        let service_name = cfg.service_name.clone();
        let group = Some(cfg.group.clone());
        
        debug!("正在注册服务实例: instanceId={}, port={}", instance_id, port);
        match naming_service.register_instance(service_name.clone(), group.clone(), instance).await {
            Ok(_) => {
                info!("✓ 成功注册服务实例到 Nacos: instanceId={}, service={}, group={:?}, ip={}, port={}", 
                    instance_id, service_name, group, local_ip, port);
                registered_count += 1;
            }
            Err(e) => {
                let error_str = e.to_string();
                error!("✗ 注册服务实例失败: instanceId={}, port={}, error={}", instance_id, port, error_str);
                
                // 如果是连接错误，提供额外的诊断信息
                if error_str.contains("Unavailable") || error_str.contains("tcp connect error") || error_str.contains("can't open server stream") {
                    error!("  这通常表示 gRPC 连接失败。请检查：");
                    error!("  1. Nacos 服务器 {} 是否可访问", cfg.server_addr);
                    error!("  2. gRPC 端口（通常是 9848 或 HTTP端口+1000）是否开放");
                    error!("  3. 防火墙规则是否允许 gRPC 连接");
                    error!("  4. Nacos 服务日志中是否有相关错误");
                }
            }
        }
    }

    if registered_count > 0 {
        info!("Nacos 注册完成: 成功注册 {}/{} 个服务实例", registered_count, ports.len());
    } else {
        warn!("Nacos 注册失败: 未能注册任何服务实例");
    }

    Ok(())
}

/// 清理集群名称，使其符合 Nacos 规范（只能包含 0-9a-zA-Z-. 字符）
fn sanitize_cluster_name(group: &str) -> String {
    // 如果 group 是 "DEFAULT_GROUP"，使用标准的 "DEFAULT"
    if group == "DEFAULT_GROUP" {
        return "DEFAULT".to_string();
    }
    
    // 将不符合规范的字符替换为连字符
    let sanitized: String = group
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '.' {
                c
            } else {
                // 将下划线、空格等替换为连字符
                '-'
            }
        })
        .collect();
    
    // 移除首尾的连字符，并处理连续连字符
    let sanitized = sanitized.trim_matches('-');
    
    // 如果结果为空，使用默认值
    if sanitized.is_empty() {
        "DEFAULT".to_string()
    } else {
        // 将连续的连字符替换为单个连字符
        sanitized
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-")
    }
}

/// 创建 NamingService（带重试机制）
async fn create_naming_service_with_retry(
    cfg: &NacosConfig,
    max_retries: u32,
) -> Result<NamingService, Box<dyn std::error::Error + Send + Sync>> {
    let mut last_error = None;
    
    for attempt in 1..=max_retries {
        // 每次重试都重新构建 ClientProps
        let mut props = ClientProps::new()
            .server_addr(cfg.server_addr.clone())
            .app_name(cfg.service_name.clone());
        
        // 设置命名空间
        if let Some(ref ns) = cfg.namespace {
            if !ns.is_empty() {
                props = props.namespace(ns.clone());
            }
        }
        
        // 设置认证信息（如果提供）
        if let (Some(ref username), Some(ref password)) = (&cfg.username, &cfg.password) {
            props = props.auth_username(username.clone()).auth_password(password.clone());
        }
        
        match NamingServiceBuilder::new(props).build().await {
            Ok(service) => {
                if attempt > 1 {
                    info!("✓ Nacos 连接成功（第 {} 次尝试）", attempt);
                }
                return Ok(service);
            }
            Err(e) => {
                let error_msg = e.to_string();
                last_error = Some(error_msg.clone());
                
                if attempt < max_retries {
                    let delay_secs = attempt as u64;
                    warn!("Nacos 连接失败（第 {} 次尝试）: {}，{} 秒后重试...", attempt, error_msg, delay_secs);
                    tokio::time::sleep(Duration::from_secs(delay_secs)).await;
                } else {
                    // 最后一次尝试失败，返回错误
                    return Err(format!("创建 Nacos NamingService 失败（已重试 {} 次）: {}", max_retries, error_msg).into());
                }
            }
        }
    }
    
    Err(format!("创建 Nacos NamingService 失败: {}", last_error.unwrap_or_else(|| "未知错误".to_string())).into())
}

/// 本机 IPv4（简单实现）
pub fn local_ip() -> String {
    let s = match std::net::UdpSocket::bind("0.0.0.0:0") {
        Ok(s) => s,
        Err(_) => return "127.0.0.1".to_string(),
    };
    if s.connect("8.8.8.8:80").is_err() {
        return "127.0.0.1".to_string();
    }
    s.local_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}
