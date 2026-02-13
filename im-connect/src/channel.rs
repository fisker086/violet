//! 用户 -> 多设备 Channel 管理：同组互斥、踢人、Redis 路由

use crate::constants;
use crate::device::{DeviceGroup, IMDeviceType};
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{debug, info};

/// 单用户某设备分组的连接：可向该连接发送二进制帧
#[derive(Clone)]
pub struct UserChannel {
    pub channel_id: String,
    #[allow(dead_code)]
    pub device_type: IMDeviceType,
    #[allow(dead_code)]
    pub group: DeviceGroup,
    pub tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// 用户 -> 设备分组 -> 连接
pub struct UserChannelMap {
    /// brokerId，用于 Redis 注册
    #[allow(dead_code)]
    pub broker_id: String,
    /// 是否允许多端同时在线
    pub multi_device_enabled: bool,
    /// user_id -> (DeviceGroup -> UserChannel)
    inner: Arc<DashMap<String, DashMap<DeviceGroup, UserChannel>>>,
}

impl UserChannelMap {
    pub fn new(broker_id: String, multi_device_enabled: bool) -> Self {
        Self {
            broker_id,
            multi_device_enabled,
            inner: Arc::new(DashMap::new()),
        }
    }

    /// 添加连接：同组互斥则踢旧连接；单端模式则踢其他所有组
    pub fn add_channel(
        &self,
        user_id: String,
        device_type: IMDeviceType,
        tx: mpsc::UnboundedSender<Vec<u8>>,
    ) {
        let channel_id = uuid::Uuid::new_v4().to_string();
        let group = device_type.group;

        let by_user = self.inner.entry(user_id.clone()).or_insert_with(DashMap::new);

        // 同组互斥
        if let Some((_, old)) = by_user.remove(&group) {
            info!(
                "同组互斥踢人: userId={}, group={:?}, oldChannelId={}, newChannelId={}",
                user_id, group, old.channel_id, channel_id
            );
            self.send_kick_and_close(old, "同类型设备登录，您已被强制下线");
        }

        if !self.multi_device_enabled {
            let to_remove: Vec<DeviceGroup> = by_user
                .iter()
                .filter(|r| *r.key() != group)
                .map(|r| *r.key())
                .collect();
            for g in to_remove {
                if let Some((_, uc)) = by_user.remove(&g) {
                    self.send_kick_and_close(uc, "账号在其他端登录，您已被强制下线");
                }
            }
        }

        let uc = UserChannel {
            channel_id: channel_id.clone(),
            device_type,
            group,
            tx,
        };
        by_user.insert(group, uc);
        info!("用户通道绑定: userId={}, group={:?}, type={}", user_id, group, device_type.type_name);
    }

    fn send_kick_and_close(&self, uc: UserChannel, reason: &str) {
        let code = constants::code::FORCE_LOGOUT;
        let msg = crate::message::wrap_to_proto_bytes(code, None, Some(reason), None);
        let _ = uc.tx.send(msg);
        drop(uc.tx);
    }

    /// 按 userId 移除某设备分组
    #[allow(dead_code)]
    pub fn remove_channel(&self, user_id: &str, device_type_str: &str, close: bool) {
        let Some(by_user) = self.inner.get_mut(user_id) else { return };
        let group = IMDeviceType::group_from(device_type_str, DeviceGroup::Web);
        if let Some((_, uc)) = by_user.remove(&group) {
            if close {
                drop(uc.tx);
            }
        }
        if by_user.is_empty() {
            drop(by_user);
            self.inner.remove(user_id);
        }
    }

    /// 连接断开时按 channel 清理（通过 channel_id 比对避免误删新连接）
    #[allow(dead_code)]
    pub fn remove_by_channel_id(&self, user_id: &str, group: DeviceGroup, channel_id: &str) {
        if let Some(by_user) = self.inner.get_mut(user_id) {
            if let Some(uc_ref) = by_user.get(&group) {
                if uc_ref.channel_id == channel_id {
                    drop(uc_ref);
                    by_user.remove(&group);
                }
            }
            if by_user.is_empty() {
                drop(by_user);
                self.inner.remove(user_id);
            }
            debug!("清理离线通道: userId={}, group={:?}", user_id, group);
        }
    }

    /// 获取用户某设备分组的发送端（用于踢人时发消息）
    #[allow(dead_code)]
    pub fn get_tx(&self, user_id: &str, device_type: IMDeviceType) -> Option<mpsc::UnboundedSender<Vec<u8>>> {
        self.inner.get(user_id).and_then(|m| {
            m.get(&device_type.group).map(|uc| uc.tx.clone())
        })
    }

    /// 获取用户所有在线的发送端（用于单聊/群聊推送）
    pub fn get_all_tx_by_user(&self, user_id: &str) -> Vec<mpsc::UnboundedSender<Vec<u8>>> {
        self.inner
            .get(user_id)
            .map(|m| m.iter().map(|r| r.value().tx.clone()).collect())
            .unwrap_or_default()
    }

    #[allow(dead_code)]
    pub fn online_user_count(&self) -> usize {
        self.inner.len()
    }

    #[allow(dead_code)]
    pub fn total_connection_count(&self) -> usize {
        self.inner.iter().map(|r| r.len()).sum()
    }
}