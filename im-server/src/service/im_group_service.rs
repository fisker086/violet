use crate::model::{ImGroup, ImGroupMember};
use crate::error::{ErrorCode, Result};
use sqlx::MySqlPool;
use im_share::now_timestamp;
use serde::Deserialize;

#[derive(Deserialize)]
pub struct UpdateGroupRequest {
    pub group_name: Option<String>,
    pub introduction: Option<String>,
    pub avatar: Option<String>,
    pub notification: Option<String>,
    pub apply_join_type: Option<i32>,
    pub max_member_count: Option<i32>,
}

pub struct ImGroupService {
    pool: MySqlPool,
}

impl ImGroupService {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }


    /// 创建群组
    pub async fn create_group(&self, group: ImGroup) -> Result<()> {
        use tracing::{error, warn};
        let now = now_timestamp();

        // 验证必填字段
        if group.group_id.is_empty() {
            warn!("群组ID为空");
            return Err(ErrorCode::InvalidInput);
        }
        if group.group_name.is_empty() {
            warn!("群组名称为空");
            return Err(ErrorCode::InvalidInput);
        }
        if group.owner_id.is_empty() {
            warn!("群主ID为空");
            return Err(ErrorCode::InvalidInput);
        }

        // 检查 group_id 长度（数据库限制为 VARCHAR(50)）
        if group.group_id.len() > 50 {
            warn!("群组ID长度超过限制: {} > 50", group.group_id.len());
            return Err(ErrorCode::InvalidInput);
        }

        // 检查 group_name 长度（数据库限制为 VARCHAR(100)）
        if group.group_name.len() > 100 {
            warn!("群组名称长度超过限制: {} > 100", group.group_name.len());
            return Err(ErrorCode::InvalidInput);
        }

        // 检查 introduction 长度（数据库限制为 VARCHAR(100)）
        if let Some(ref intro) = group.introduction {
            if intro.len() > 100 {
                warn!("群组简介长度超过限制: {} > 100", intro.len());
                return Err(ErrorCode::InvalidInput);
            }
        }

        let result = sqlx::query(
            "INSERT INTO im_group 
             (group_id, owner_id, group_type, group_name, mute, apply_join_type, avatar, 
              max_member_count, introduction, notification, status, sequence, create_time, 
              update_time, extra, version, del_flag, verifier) 
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, 0, ?, ?, ?, 1, 1, ?)"
        )
        .bind(&group.group_id)
        .bind(&group.owner_id)
        .bind(group.group_type)
        .bind(&group.group_name)
        .bind(&group.mute)
        .bind(group.apply_join_type)
        .bind(&group.avatar)
        .bind(&group.max_member_count)
        .bind(&group.introduction)
        .bind(&group.notification)
        .bind(now)
        .bind(now)
        .bind(&group.extra)
        .bind(&group.verifier)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => {
                // 添加群主为成员
                if let Err(e) = self.add_group_member(&group.group_id, &group.owner_id, 2, None).await {
                    error!("添加群主为成员失败: {:?}", e);
                    return Err(e);
                }
                Ok(())
            },
            Err(e) => {
                error!("创建群组数据库错误: {:?}", e);
                // 检查是否是重复键错误
                if let sqlx::Error::Database(db_err) = &e {
                    if db_err.message().contains("Duplicate entry") || db_err.message().contains("PRIMARY") {
                        warn!("群组ID已存在: {}", group.group_id);
                        return Err(ErrorCode::InvalidInput);
                    }
                }
                Err(ErrorCode::Database)
            }
        }
    }

    /// 获取群组信息
    pub async fn get_group(&self, group_id: &str) -> Result<ImGroup> {
        let group = sqlx::query_as::<_, ImGroup>(
            "SELECT group_id, owner_id, group_type, group_name, mute, apply_join_type, avatar, 
                    max_member_count, introduction, notification, status, sequence, create_time, 
                    update_time, extra, version, del_flag, verifier,
                    (SELECT COUNT(*) FROM im_group_member gm WHERE gm.group_id = im_group.group_id AND gm.del_flag = 1) as member_count
             FROM im_group 
             WHERE group_id = ? AND del_flag = 1"
        )
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        match group {
            Some(g) => Ok(g),
            None => Err(ErrorCode::NotFound),
        }
    }

    /// 添加群成员
    pub async fn add_group_member(&self, group_id: &str, member_id: &str, role: i32, alias: Option<String>) -> Result<()> {
        use tracing::{error, warn};
        let now = now_timestamp();
        let group_member_id = format!("{}_{}", group_id, member_id);
        
        // 验证 group_member_id 长度（通常数据库限制为 VARCHAR(100) 或类似）
        if group_member_id.len() > 100 {
            warn!("群成员ID长度超过限制: {} > 100, group_id={}, member_id={}", 
                  group_member_id.len(), group_id, member_id);
            return Err(ErrorCode::InvalidInput);
        }

        let result = sqlx::query(
            "INSERT INTO im_group_member 
             (group_member_id, group_id, member_id, role, mute, alias, join_time, del_flag, 
              create_time, update_time, version) 
             VALUES (?, ?, ?, ?, 1, ?, ?, 1, ?, ?, 1)
             ON DUPLICATE KEY UPDATE 
             role = VALUES(role), alias = VALUES(alias), del_flag = 1, update_time = ?, version = version + 1"
        )
        .bind(&group_member_id)
        .bind(group_id)
        .bind(member_id)
        .bind(role)
        .bind(&alias)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("添加群成员数据库错误: group_id={}, member_id={}, group_member_id={}, error={:?}", 
                       group_id, member_id, group_member_id, e);
                // 检查是否是外键约束错误
                if let sqlx::Error::Database(db_err) = &e {
                    let error_msg = db_err.message();
                    let _error_code = db_err.code().as_deref();
                    
                    // 检查是否是字段长度错误
                    if error_msg.contains("Data too long") || error_msg.contains("too long for column") {
                        error!("group_member_id 字段长度不足: group_member_id={} (长度: {}), 请执行 fix_group_member_id_length.sql 修复数据库字段长度", 
                               group_member_id, group_member_id.len());
                        return Err(ErrorCode::Database);
                    }
                    
                    // 检查是否是外键约束错误
                    if error_msg.contains("foreign key constraint") || error_msg.contains("FOREIGN KEY") {
                        warn!("外键约束错误: 群组 {} 或成员 {} 可能不存在", group_id, member_id);
                        return Err(ErrorCode::NotFound);
                    }
                    
                    // 检查是否是唯一约束错误
                    if error_msg.contains("Duplicate entry") || error_msg.contains("PRIMARY") {
                        warn!("群成员记录已存在: group_id={}, member_id={}", group_id, member_id);
                        // 对于重复键，ON DUPLICATE KEY UPDATE 应该已经处理，但如果还是失败，返回错误
                        return Err(ErrorCode::InvalidInput);
                    }
                }
                Err(ErrorCode::Database)
            }
        }
    }

    /// 获取群成员列表
    pub async fn get_group_members(&self, group_id: &str) -> Result<Vec<ImGroupMember>> {
        // 获取所有群成员记录（可能包含重复的 member_id）
        // 在应用层进行去重，确保每个 member_id 只返回一条记录
        let all_members = sqlx::query_as::<_, ImGroupMember>(
            "SELECT group_member_id, group_id, member_id, role, speak_date, mute, alias, 
                    join_time, leave_time, join_type, extra, del_flag, create_time, update_time, version 
             FROM im_group_member 
             WHERE group_id = ? AND del_flag = 1
             ORDER BY role DESC, update_time DESC, join_time ASC"
        )
        .bind(group_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        // 使用 HashMap 去重，保留每个 member_id 的第一条记录（已按 update_time DESC 排序，所以是最新的）
        use std::collections::HashMap;
        let mut unique_members = HashMap::new();
        for member in all_members {
            unique_members.entry(member.member_id.clone()).or_insert(member);
        }

        // 转换为 Vec 并重新排序
        let mut members: Vec<ImGroupMember> = unique_members.into_values().collect();
        members.sort_by(|a, b| {
            // 先按角色排序（角色高的在前），然后按加入时间排序
            match b.role.cmp(&a.role) {
                std::cmp::Ordering::Equal => {
                    a.join_time.unwrap_or(0).cmp(&b.join_time.unwrap_or(0))
                }
                other => other,
            }
        });

        Ok(members)
    }

    /// 移除群成员（只有群主和管理员可以移除成员）
    pub async fn remove_group_member(&self, group_id: &str, member_id: &str, operator_id: &str) -> Result<()> {
        use tracing::{warn, error};
        let now = now_timestamp();

        // 验证群组是否存在
        let group = match self.get_group(group_id).await {
            Ok(g) => g,
            Err(e) => {
                warn!("群组不存在: {}", group_id);
                return Err(e);
            }
        };

        // 验证操作者权限：只有群主或管理员可以移除成员
        let members = match self.get_group_members(group_id).await {
            Ok(m) => m,
            Err(e) => {
                warn!("获取群成员列表失败: group_id={}, error={:?}", group_id, e);
                return Err(e);
            }
        };

        // 查找操作者的成员信息
        let operator_member = members.iter().find(|m| m.member_id == operator_id);
        let is_owner = group.owner_id.trim() == operator_id.trim();
        let is_admin = operator_member.map(|m| m.role == 1).unwrap_or(false);

        if !is_owner && !is_admin {
            warn!("用户 {} 不是群主或管理员，无法移除成员: group_id={}", operator_id, group_id);
            return Err(ErrorCode::InvalidInput);
        }

        // 不能移除群主
        if member_id.trim() == group.owner_id.trim() {
            warn!("不能移除群主: group_id={}, member_id={}", group_id, member_id);
            return Err(ErrorCode::InvalidInput);
        }

        // 查找要移除的成员
        let target_member = members.iter().find(|m| m.member_id == member_id);
        if target_member.is_none() {
            warn!("要移除的成员不存在: group_id={}, member_id={}", group_id, member_id);
            return Err(ErrorCode::NotFound);
        }

        // 管理员不能移除其他管理员（只有群主可以）
        if !is_owner {
            if let Some(target) = target_member {
                if target.role == 1 {
                    warn!("管理员不能移除其他管理员: group_id={}, operator_id={}, member_id={}", 
                          group_id, operator_id, member_id);
                    return Err(ErrorCode::InvalidInput);
                }
            }
        }

        // 执行删除
        sqlx::query(
            "UPDATE im_group_member 
             SET del_flag = 0, leave_time = ?, update_time = ?, version = version + 1 
             WHERE group_id = ? AND member_id = ? AND del_flag = 1"
        )
        .bind(now)
        .bind(now)
        .bind(group_id)
        .bind(member_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("移除群成员失败: group_id={}, member_id={}, error={:?}", group_id, member_id, e);
            ErrorCode::Database
        })?;

        Ok(())
    }

    /// 更新群成员角色（设置/取消管理员）
    pub async fn update_member_role(&self, group_id: &str, member_id: &str, role: i32, operator_id: &str) -> Result<()> {
        use tracing::{error, warn};
        let now = now_timestamp();

        // 验证操作者权限（只有群主可以设置/取消管理员）
        let group = match self.get_group(group_id).await {
            Ok(g) => g,
            Err(e) => {
                warn!("群组不存在: {}", group_id);
                return Err(e);
            }
        };

        if group.owner_id != operator_id {
            warn!("用户 {} 不是群组 {} 的群主，无法修改成员角色", operator_id, group_id);
            return Err(ErrorCode::InvalidInput);
        }

        // 验证角色值（0=普通成员，1=管理员，2=群主）
        if role < 0 || role > 2 {
            warn!("无效的角色值: {}", role);
            return Err(ErrorCode::InvalidInput);
        }

        // 不能修改群主的角色
        if member_id == group.owner_id && role != 2 {
            warn!("不能修改群主的角色");
            return Err(ErrorCode::InvalidInput);
        }

        // 更新成员角色
        sqlx::query(
            "UPDATE im_group_member 
             SET role = ?, update_time = ?, version = version + 1 
             WHERE group_id = ? AND member_id = ? AND del_flag = 1"
        )
        .bind(role)
        .bind(now)
        .bind(group_id)
        .bind(member_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("更新群成员角色失败: group_id={}, member_id={}, role={}, error={:?}", 
                   group_id, member_id, role, e);
            ErrorCode::Database
        })?;

        Ok(())
    }

    /// 删除群组（硬删除，只有群主可以删除）
    pub async fn delete_group(&self, group_id: &str, owner_id: &str) -> Result<()> {
        use tracing::{error, warn};

        // 首先验证是否是群主
        let group = match self.get_group(group_id).await {
            Ok(g) => g,
            Err(e) => {
                warn!("群组不存在: {}", group_id);
                return Err(e);
            }
        };

        // 去除空格进行比较（更宽松的匹配）
        let group_owner_id_trimmed = group.owner_id.trim();
        let owner_id_trimmed = owner_id.trim();
        
        if group_owner_id_trimmed != owner_id_trimmed {
            warn!(
                "用户不是群主，无法删除群组: group_id={}, group_owner_id='{}', current_owner_id='{}'",
                group_id, 
                group_owner_id_trimmed, 
                owner_id_trimmed
            );
            return Err(ErrorCode::InvalidInput);
        }

        // 先删除所有群成员（硬删除）
        sqlx::query(
            "DELETE FROM im_group_member WHERE group_id = ?"
        )
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("删除群成员失败: group_id={}, error={:?}", group_id, e);
            ErrorCode::Database
        })?;

        // 删除群组（硬删除）
        sqlx::query(
            "DELETE FROM im_group WHERE group_id = ?"
        )
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("删除群组失败: group_id={}, error={:?}", group_id, e);
            ErrorCode::Database
        })?;

        Ok(())
    }

    /// 解散群组（只有群主可以解散）
    /// 返回解散前的成员列表，用于发送系统消息
    pub async fn dissolve_group(&self, group_id: &str, owner_id: &str) -> Result<Vec<ImGroupMember>> {
        use tracing::{error, warn, info};
        let now = now_timestamp();

        // 首先检查群组是否存在（无论 del_flag 状态）
        let group = sqlx::query_as::<_, ImGroup>(
            "SELECT group_id, owner_id, group_type, group_name, mute, apply_join_type, avatar, 
                    max_member_count, introduction, notification, status, sequence, create_time, 
                    update_time, extra, version, del_flag, verifier,
                    (SELECT COUNT(*) FROM im_group_member gm WHERE gm.group_id = im_group.group_id AND gm.del_flag = 1) as member_count
             FROM im_group 
             WHERE group_id = ?"
        )
        .bind(group_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!("查询群组失败: group_id={}, error={:?}", group_id, e);
            ErrorCode::Database
        })?;

        let group = match group {
            Some(g) => g,
            None => {
                warn!("群组不存在: {}", group_id);
                return Err(ErrorCode::NotFound);
            }
        };

        // 如果群组已经解散（del_flag = 0），直接返回成功（幂等操作）
        if group.del_flag == 0 {
            info!("群组已经解散，无需重复操作: group_id={}", group_id);
            return Ok(vec![]);
        }

        // 验证是否是群主
        let group_owner_id_trimmed = group.owner_id.trim();
        let owner_id_trimmed = owner_id.trim();
        
        if group_owner_id_trimmed != owner_id_trimmed {
            warn!(
                "用户不是群主，无法解散群组: group_id={}, group_owner_id='{}', current_owner_id='{}', group_owner_id_len={}, owner_id_len={}",
                group_id, 
                group_owner_id_trimmed, 
                owner_id_trimmed,
                group_owner_id_trimmed.len(),
                owner_id_trimmed.len()
            );
            return Err(ErrorCode::InvalidInput);
        }

        // 在解散前获取所有成员列表（用于发送系统消息）
        let members = match self.get_group_members(group_id).await {
            Ok(m) => m,
            Err(e) => {
                warn!("获取群成员列表失败: group_id={}, error={:?}", group_id, e);
                vec![] // 即使获取成员失败，也继续解散群组
            }
        };

        // 软删除群组（设置 del_flag = 0）
        sqlx::query(
            "UPDATE im_group 
             SET del_flag = 0, update_time = ?, version = version + 1 
             WHERE group_id = ?"
        )
        .bind(now)
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("解散群组数据库错误: group_id={}, error={:?}", group_id, e);
            ErrorCode::Database
        })?;

        // 同时软删除所有群成员（设置 del_flag = 0）
        sqlx::query(
            "UPDATE im_group_member 
             SET del_flag = 0, leave_time = ?, update_time = ?, version = version + 1 
             WHERE group_id = ?"
        )
        .bind(now)
        .bind(now)
        .bind(group_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("解散群组时删除成员失败: group_id={}, error={:?}", group_id, e);
            ErrorCode::Database
        })?;

        // 返回解散前的成员列表
        Ok(members)
    }

    /// 更新群成员别名（我在本群的昵称）
    pub async fn update_member_alias(&self, group_id: &str, member_id: &str, alias: Option<String>) -> Result<()> {
        use tracing::{error, warn};
        let now = now_timestamp();

        // 验证成员是否存在
        let member = match self.get_group_members(group_id).await {
            Ok(members) => {
                members.iter().find(|m| m.member_id == member_id).cloned()
            }
            Err(e) => {
                warn!("获取群成员列表失败: group_id={}, error={:?}", group_id, e);
                return Err(e);
            }
        };

        if member.is_none() {
            warn!("群成员不存在: group_id={}, member_id={}", group_id, member_id);
            return Err(ErrorCode::NotFound);
        }

        // 更新成员别名
        sqlx::query(
            "UPDATE im_group_member 
             SET alias = ?, update_time = ?, version = version + 1 
             WHERE group_id = ? AND member_id = ? AND del_flag = 1"
        )
        .bind(&alias)
        .bind(now)
        .bind(group_id)
        .bind(member_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("更新群成员别名失败: group_id={}, member_id={}, alias={:?}, error={:?}", 
                   group_id, member_id, alias, e);
            ErrorCode::Database
        })?;

        Ok(())
    }

    /// 更新群组信息（只有群主可以更新）
    pub async fn update_group(&self, group_id: &str, owner_id: &str, req: &UpdateGroupRequest) -> Result<()> {
        use tracing::{error, warn};
        let now = now_timestamp();

        // 验证是否是群主
        let group = match self.get_group(group_id).await {
            Ok(g) => g,
            Err(e) => {
                warn!("群组不存在: {}", group_id);
                return Err(e);
            }
        };

        // 去除空格进行比较（更宽松的匹配）
        let group_owner_id_trimmed = group.owner_id.trim();
        let owner_id_trimmed = owner_id.trim();
        
        if group_owner_id_trimmed != owner_id_trimmed {
            warn!(
                "用户不是群主，无法更新群组信息: group_id={}, group_owner_id='{}', current_owner_id='{}'",
                group_id, 
                group_owner_id_trimmed, 
                owner_id_trimmed
            );
            return Err(ErrorCode::InvalidInput);
        }

        // 使用 QueryBuilder 构建动态SQL，完全手动控制逗号
        let mut query_builder = sqlx::QueryBuilder::new("UPDATE im_group SET ");
        let mut has_update = false;
        let mut need_comma = false;

        if let Some(ref group_name) = req.group_name {
            if group_name.trim().is_empty() {
                warn!("群组名称不能为空");
                return Err(ErrorCode::InvalidInput);
            }
            if group_name.len() > 100 {
                warn!("群组名称长度超过限制: {} > 100", group_name.len());
                return Err(ErrorCode::InvalidInput);
            }
            if need_comma {
                query_builder.push(", ");
            }
            query_builder.push("group_name = ");
            query_builder.push_bind(group_name.trim().to_string());
            need_comma = true;
            has_update = true;
        }
        if let Some(ref introduction) = req.introduction {
            if introduction.len() > 100 {
                warn!("群组简介长度超过限制: {} > 100", introduction.len());
                return Err(ErrorCode::InvalidInput);
            }
            if need_comma {
                query_builder.push(", ");
            }
            query_builder.push("introduction = ");
            query_builder.push_bind(introduction.trim().to_string());
            need_comma = true;
            has_update = true;
        }
        if let Some(ref avatar) = req.avatar {
            if need_comma {
                query_builder.push(", ");
            }
            query_builder.push("avatar = ");
            query_builder.push_bind(avatar.trim().to_string());
            need_comma = true;
            has_update = true;
        }
        if let Some(ref notification) = req.notification {
            if need_comma {
                query_builder.push(", ");
            }
            query_builder.push("notification = ");
            query_builder.push_bind(notification.trim().to_string());
            need_comma = true;
            has_update = true;
        }
        if let Some(apply_join_type) = req.apply_join_type {
            if need_comma {
                query_builder.push(", ");
            }
            query_builder.push("apply_join_type = ");
            query_builder.push_bind(apply_join_type);
            need_comma = true;
            has_update = true;
        }
        if let Some(max_member_count) = req.max_member_count {
            if max_member_count < 1 {
                warn!("最大成员数必须大于0");
                return Err(ErrorCode::InvalidInput);
            }
            if need_comma {
                query_builder.push(", ");
            }
            query_builder.push("max_member_count = ");
            query_builder.push_bind(max_member_count);
            need_comma = true;
            has_update = true;
        }

        if !has_update {
            warn!("没有需要更新的字段");
            return Err(ErrorCode::InvalidInput);
        }

        // 添加 update_time 和 version（这些总是需要更新的）
        if need_comma {
            query_builder.push(", ");
        }
        query_builder.push("update_time = ");
        query_builder.push_bind(now);
        query_builder.push(", version = version + 1");

        query_builder.push(" WHERE group_id = ");
        query_builder.push_bind(group_id);
        query_builder.push(" AND del_flag = 1");

        query_builder.build()
            .execute(&self.pool)
            .await
            .map_err(|e| {
                error!("更新群组信息失败: group_id={}, error={:?}", group_id, e);
                ErrorCode::Database
            })?;

        Ok(())
    }

    /// 获取用户所在的群组列表
    /// 注意：只有3人及以上的群组才会在 im_group 表中有记录
    /// 2人聊天不会创建群组记录，所以这里只返回真正的群组（3人及以上）
    pub async fn get_user_groups(&self, user_id: &str) -> Result<Vec<ImGroup>> {
        let groups = sqlx::query_as::<_, ImGroup>(
            "SELECT g.group_id, g.owner_id, g.group_type, g.group_name, g.mute, g.apply_join_type, 
                    g.avatar, g.max_member_count, g.introduction, g.notification, g.status, 
                    g.sequence, g.create_time, g.update_time, g.extra, g.version, g.del_flag, 
                    g.verifier,
                    (SELECT COUNT(*) FROM im_group_member gm WHERE gm.group_id = g.group_id AND gm.del_flag = 1) as member_count
             FROM im_group g
             INNER JOIN im_group_member gm ON g.group_id = gm.group_id
             WHERE gm.member_id = ? AND g.del_flag = 1 AND gm.del_flag = 1
             HAVING member_count >= 3
             ORDER BY g.update_time DESC"
        )
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            use tracing::error;
            error!("获取用户群组列表失败: user_id={}, error={:?}", user_id, e);
            ErrorCode::Database
        })?;

        Ok(groups)
    }
}

