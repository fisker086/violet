use crate::model::{ImFriendship, ImFriendshipRequest};
use crate::error::{ErrorCode, Result};
use sqlx::{MySqlPool, Row};
use im_share::now_timestamp;
use tracing::{info, warn, debug};

pub struct ImFriendshipService {
    pool: MySqlPool,
}

impl ImFriendshipService {
    pub fn new(pool: MySqlPool) -> Self {
        Self { pool }
    }


    /// 检查是否是好友
    /// 检查双向关系（owner_id -> to_id 或 to_id -> owner_id）
    pub async fn is_friend(&self, owner_id: &str, to_id: &str) -> Result<bool> {
        debug!("检查好友关系: owner_id={}, to_id={}", owner_id, to_id);
        
        // 先尝试直接匹配（检查双向关系）
        let result = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM im_friendship 
             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
             AND (del_flag IS NULL OR del_flag = 1) 
             AND (black IS NULL OR black = 1)"
        )
        .bind(owner_id)
        .bind(to_id)
        .bind(to_id)
        .bind(owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            warn!("检查好友关系失败: owner_id={}, to_id={}, error={:?}", owner_id, to_id, e);
            ErrorCode::Database
        })?;

        let is_friend = result > 0;
        debug!("好友关系检查结果（直接匹配）: owner_id={}, to_id={}, is_friend={}", owner_id, to_id, is_friend);
        
        // 如果直接匹配失败，可能是ID格式不一致（比如一个是open_id，一个是用户名）
        // 尝试通过用户表查找可能的匹配，然后使用所有可能的标识组合查询
        if !is_friend {
            // 使用一个查询同时查找两个用户的所有标识
            let user_row = sqlx::query(
                "SELECT 
                    u1.open_id as owner_open_id, u1.name as owner_name, u1.phone as owner_phone,
                    u2.open_id as to_open_id, u2.name as to_name, u2.phone as to_phone
                 FROM users u1, users u2
                 WHERE (u1.open_id = ? OR u1.name = ? OR u1.phone = ?)
                 AND (u2.open_id = ? OR u2.name = ? OR u2.phone = ?)
                 AND (u1.status IS NULL OR u1.status = 1)
                 AND (u2.status IS NULL OR u2.status = 1)
                 LIMIT 1"
            )
            .bind(owner_id)
            .bind(owner_id)
            .bind(owner_id)
            .bind(to_id)
            .bind(to_id)
            .bind(to_id)
            .fetch_optional(&self.pool)
            .await
            .ok()
            .flatten();
            
            // 如果找到了两个用户的标识，尝试所有可能的组合
            if let Some(row) = user_row {
                let owner_open_id: Option<String> = row.try_get("owner_open_id").ok().flatten();
                let owner_name: Option<String> = row.try_get("owner_name").ok().flatten();
                let owner_phone: Option<String> = row.try_get("owner_phone").ok().flatten();
                let to_open_id: Option<String> = row.try_get("to_open_id").ok().flatten();
                let to_name: Option<String> = row.try_get("to_name").ok().flatten();
                let to_phone: Option<String> = row.try_get("to_phone").ok().flatten();
                
                // 收集所有可能的标识（去重）
                let mut owner_ids: Vec<String> = Vec::new();
                if let Some(ref id) = owner_open_id {
                    if id != owner_id {
                        owner_ids.push(id.clone());
                    }
                }
                if let Some(ref id) = owner_name {
                    if id != owner_id && !owner_ids.contains(id) {
                        owner_ids.push(id.clone());
                    }
                }
                if let Some(ref id) = owner_phone {
                    if id != owner_id && !owner_ids.contains(id) {
                        owner_ids.push(id.clone());
                    }
                }
                
                let mut to_ids: Vec<String> = Vec::new();
                if let Some(ref id) = to_open_id {
                    if id != to_id {
                        to_ids.push(id.clone());
                    }
                }
                if let Some(ref id) = to_name {
                    if id != to_id && !to_ids.contains(id) {
                        to_ids.push(id.clone());
                    }
                }
                if let Some(ref id) = to_phone {
                    if id != to_id && !to_ids.contains(id) {
                        to_ids.push(id.clone());
                    }
                }
                
                // 尝试所有可能的组合（包括原始ID）
                let all_owner_ids: Vec<&str> = {
                    let mut ids = vec![owner_id];
                    ids.extend(owner_ids.iter().map(|s| s.as_str()));
                    ids
                };
                
                let all_to_ids: Vec<&str> = {
                    let mut ids = vec![to_id];
                    ids.extend(to_ids.iter().map(|s| s.as_str()));
                    ids
                };
                
                // 尝试所有可能的组合
                for oid in &all_owner_ids {
                    for tid in &all_to_ids {
                        if *oid == owner_id && *tid == to_id {
                            continue; // 已经检查过了
                        }
                        
                        let check_result = sqlx::query_scalar::<_, i64>(
                            "SELECT COUNT(*) FROM im_friendship 
                             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
                             AND (del_flag IS NULL OR del_flag = 1) 
                             AND (black IS NULL OR black = 1)"
                        )
                        .bind(oid)
                        .bind(tid)
                        .bind(tid)
                        .bind(oid)
                        .fetch_optional(&self.pool)
                        .await
                        .ok()
                        .flatten()
                        .unwrap_or(0);
                        
                        if check_result > 0 {
                            debug!("好友关系检查结果（通过ID转换匹配）: owner_id={}, to_id={}, matched_owner={}, matched_to={}, is_friend=true", 
                                   owner_id, to_id, oid, tid);
                            return Ok(true);
                        }
                    }
                }
            }
        }
        
        Ok(is_friend)
    }

    /// 获取好友列表
    /// 支持通过用户名、手机号或 open_id 查询
    pub async fn get_friends(&self, owner_id: &str) -> Result<Vec<ImFriendship>> {
        debug!("查询好友列表: owner_id={}", owner_id);
        
        // 首先尝试直接匹配
        let friends = sqlx::query_as::<_, ImFriendship>(
            "SELECT owner_id, to_id, remark, del_flag, black, create_time, update_time, 
                    sequence, black_sequence, add_source, extra, version 
             FROM im_friendship 
             WHERE owner_id = ? AND (del_flag IS NULL OR del_flag = 1) 
             AND (black IS NULL OR black = 1)
             ORDER BY sequence DESC, create_time DESC"
        )
        .bind(owner_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            warn!("查询好友列表失败: owner_id={}, error={:?}", owner_id, e);
            ErrorCode::Database
        })?;

        if !friends.is_empty() {
            info!("查询好友列表结果（直接匹配）: owner_id={}, count={}", owner_id, friends.len());
            return Ok(friends);
        }

        // 如果直接匹配没有结果，尝试通过用户表查找可能的 owner_id 格式
        // 先尝试作为用户名查找
        let user_by_name = sqlx::query_scalar::<_, Option<String>>(
            "SELECT open_id FROM users WHERE name = ? AND (status IS NULL OR status = 1) LIMIT 1"
        )
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten();

        // 如果找到用户，尝试用 open_id 查询
        if let Some(open_id) = user_by_name {
            if open_id != owner_id {
                debug!("尝试使用 open_id 查询好友列表: open_id={}", open_id);
                let friends_by_open_id = sqlx::query_as::<_, ImFriendship>(
                    "SELECT owner_id, to_id, remark, del_flag, black, create_time, update_time, 
                            sequence, black_sequence, add_source, extra, version 
                     FROM im_friendship 
                     WHERE owner_id = ? AND (del_flag IS NULL OR del_flag = 1) 
                     AND (black IS NULL OR black = 1)
                     ORDER BY sequence DESC, create_time DESC"
                )
                .bind(&open_id)
                .fetch_all(&self.pool)
                .await
                .ok();

                if let Some(friends) = friends_by_open_id {
                    if !friends.is_empty() {
                        warn!("使用 open_id 查询到好友列表: owner_id={}, open_id={}, count={}", 
                              owner_id, open_id, friends.len());
                        return Ok(friends);
                    }
                }
            }
        }

        // 尝试作为 open_id 查找对应的用户名
        let user_by_open_id = sqlx::query_scalar::<_, Option<String>>(
            "SELECT name FROM users WHERE open_id = ? AND (status IS NULL OR status = 1) LIMIT 1"
        )
        .bind(owner_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .flatten();

        if let Some(name) = user_by_open_id {
            if name != owner_id && !name.is_empty() {
                debug!("尝试使用用户名查询好友列表: name={}", name);
                let friends_by_name = sqlx::query_as::<_, ImFriendship>(
                    "SELECT owner_id, to_id, remark, del_flag, black, create_time, update_time, 
                            sequence, black_sequence, add_source, extra, version 
                     FROM im_friendship 
                     WHERE owner_id = ? AND (del_flag IS NULL OR del_flag = 1) 
                     AND (black IS NULL OR black = 1)
                     ORDER BY sequence DESC, create_time DESC"
                )
                .bind(&name)
                .fetch_all(&self.pool)
                .await
                .ok();

                if let Some(friends) = friends_by_name {
                    if !friends.is_empty() {
                        warn!("使用用户名查询到好友列表: owner_id={}, name={}, count={}", 
                              owner_id, name, friends.len());
                        return Ok(friends);
                    }
                }
            }
        }

        info!("查询好友列表结果: owner_id={}, count=0", owner_id);
        Ok(friends)
    }

    /// 添加好友（双向关系）
    pub async fn add_friend(&self, owner_id: String, to_id: String, add_source: Option<String>, remark: Option<String>) -> Result<()> {
        if owner_id == to_id {
            return Err(ErrorCode::InvalidInput);
        }

        // 检查是否已经是双向好友（两个方向都检查）
        let is_owner_to_friend = self.is_friend(&owner_id, &to_id).await?;
        let is_friend_to_owner = self.is_friend(&to_id, &owner_id).await?;
        
        // 如果两个方向都是好友，说明已经是双向好友关系
        if is_owner_to_friend && is_friend_to_owner {
            return Err(ErrorCode::InvalidInput);
        }

        let now = now_timestamp();

        // 插入双向关系（如果不存在则插入，存在则更新）
        sqlx::query(
            "INSERT INTO im_friendship 
             (owner_id, to_id, remark, del_flag, black, create_time, update_time, sequence, add_source, version) 
             VALUES (?, ?, ?, 1, 1, ?, ?, ?, ?, 1)
             ON DUPLICATE KEY UPDATE 
             del_flag = 1, black = 1, remark = VALUES(remark), update_time = ?, version = version + 1"
        )
        .bind(&owner_id)
        .bind(&to_id)
        .bind(&remark)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(&add_source)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        // 插入反向关系（如果不存在则插入，存在则更新）
        sqlx::query(
            "INSERT INTO im_friendship 
             (owner_id, to_id, remark, del_flag, black, create_time, update_time, sequence, add_source, version) 
             VALUES (?, ?, ?, 1, 1, ?, ?, ?, ?, 1)
             ON DUPLICATE KEY UPDATE 
             del_flag = 1, black = 1, remark = VALUES(remark), update_time = ?, version = version + 1"
        )
        .bind(&to_id)
        .bind(&owner_id)
        .bind(&remark)
        .bind(now)
        .bind(now)
        .bind(now)
        .bind(&add_source)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 删除好友
    pub async fn remove_friend(&self, owner_id: &str, to_id: &str) -> Result<()> {
        let now = now_timestamp();

        // 先检查是否存在好友关系
        let existing_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM im_friendship 
             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
             AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(owner_id)
        .bind(to_id)
        .bind(to_id)
        .bind(owner_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            warn!("检查好友关系失败: owner_id={}, to_id={}, error={:?}", owner_id, to_id, e);
            ErrorCode::Database
        })?;

        if existing_count == 0 {
            warn!("好友关系不存在: owner_id={}, to_id={}", owner_id, to_id);
            return Err(ErrorCode::NotFound);
        }

        // 软删除双向关系
        let result = sqlx::query(
            "UPDATE im_friendship 
             SET del_flag = 0, update_time = ?, version = version + 1 
             WHERE ((owner_id = ? AND to_id = ?) OR (owner_id = ? AND to_id = ?))
             AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(now)
        .bind(owner_id)
        .bind(to_id)
        .bind(to_id)
        .bind(owner_id)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            warn!("删除好友关系失败: owner_id={}, to_id={}, error={:?}", owner_id, to_id, e);
            ErrorCode::Database
        })?;

        // 验证删除是否成功
        if result.rows_affected() == 0 {
            warn!("删除好友关系时没有更新任何记录: owner_id={}, to_id={}", owner_id, to_id);
            return Err(ErrorCode::NotFound);
        }

        info!("成功删除好友关系: owner_id={}, to_id={}, rows_affected={}", owner_id, to_id, result.rows_affected());
        Ok(())
    }

    /// 更新好友备注
    pub async fn update_remark(&self, owner_id: &str, to_id: &str, remark: Option<String>) -> Result<()> {
        let now = now_timestamp();

        sqlx::query(
            "UPDATE im_friendship 
             SET remark = ?, update_time = ?, version = version + 1 
             WHERE owner_id = ? AND to_id = ? 
             AND (del_flag IS NULL OR del_flag = 1)"
        )
        .bind(&remark)
        .bind(now)
        .bind(owner_id)
        .bind(to_id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 拉黑/取消拉黑好友（切换状态）
    pub async fn black_friend(&self, owner_id: &str, to_id: &str) -> Result<()> {
        let now = now_timestamp();

        // 先查询当前状态
        let current_black: Option<i32> = sqlx::query_scalar::<_, Option<i32>>(
            "SELECT black FROM im_friendship WHERE owner_id = ? AND to_id = ?"
        )
        .bind(owner_id)
        .bind(to_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?
        .flatten();

        let new_black = if current_black == Some(2) {
            // 如果已经拉黑，则取消拉黑
            1
        } else {
            // 否则拉黑
            2
        };

        sqlx::query(
            "UPDATE im_friendship 
             SET black = ?, black_sequence = ?, update_time = ?, version = version + 1 
             WHERE owner_id = ? AND to_id = ?"
        )
        .bind(new_black)
        .bind(if new_black == 2 { Some(now) } else { None })
        .bind(now)
        .bind(owner_id)
        .bind(to_id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        Ok(())
    }

    /// 创建好友请求
    pub async fn create_friendship_request(&self, request: ImFriendshipRequest) -> Result<()> {
        use tracing::{error, warn};
        
        // 验证必填字段
        if request.id.is_empty() {
            warn!("好友请求ID为空");
            return Err(ErrorCode::InvalidInput);
        }
        if request.from_id.is_empty() {
            warn!("发送者ID为空");
            return Err(ErrorCode::InvalidInput);
        }
        if request.to_id.is_empty() {
            warn!("接收者ID为空");
            return Err(ErrorCode::InvalidInput);
        }
        if request.from_id == request.to_id {
            warn!("不能向自己发送好友请求: from_id={}, to_id={}", request.from_id, request.to_id);
            return Err(ErrorCode::InvalidInput);
        }
        
        // 验证字段长度（假设数据库限制）
        if request.id.len() > 100 {
            warn!("好友请求ID长度超过限制: {} > 100", request.id.len());
            return Err(ErrorCode::InvalidInput);
        }
        if request.from_id.len() > 100 {
            warn!("发送者ID长度超过限制: {} > 100", request.from_id.len());
            return Err(ErrorCode::InvalidInput);
        }
        if request.to_id.len() > 100 {
            warn!("接收者ID长度超过限制: {} > 100", request.to_id.len());
            return Err(ErrorCode::InvalidInput);
        }
        if let Some(ref remark) = request.remark {
            if remark.len() > 100 {
                warn!("备注长度超过限制: {} > 100", remark.len());
                return Err(ErrorCode::InvalidInput);
            }
        }
        if let Some(ref message) = request.message {
            if message.len() > 500 {
                warn!("验证消息长度超过限制: {} > 500", message.len());
                return Err(ErrorCode::InvalidInput);
            }
        }
        if let Some(ref add_source) = request.add_source {
            if add_source.len() > 100 {
                warn!("添加来源长度超过限制: {} > 100", add_source.len());
                return Err(ErrorCode::InvalidInput);
            }
        }

        // 检查是否已经是好友
        if self.is_friend(&request.from_id, &request.to_id).await? {
            warn!("用户 {} 和 {} 已经是好友", request.from_id, request.to_id);
            return Err(ErrorCode::InvalidInput);
        }

        // 检查是否已经有待处理的好友请求（只检查待处理的，已拒绝的可以重新发送）
        let existing_requests = self.get_friendship_requests(&request.to_id, Some(0)).await?;
        if existing_requests.iter().any(|r| r.from_id == request.from_id && r.approve_status == Some(0)) {
            warn!("已经存在待处理的好友请求: from_id={}, to_id={}", request.from_id, request.to_id);
            return Err(ErrorCode::InvalidInput);
        }

        let now = now_timestamp();

        // 如果之前有被拒绝的请求，先删除它，然后创建新请求
        // 这样可以避免重复键冲突，同时允许重新发送被拒绝的请求
        sqlx::query(
            "DELETE FROM im_friendship_request 
             WHERE from_id = ? AND to_id = ? AND approve_status = 2"
        )
        .bind(&request.from_id)
        .bind(&request.to_id)
        .execute(&self.pool)
        .await
        .ok(); // 忽略删除错误（可能没有旧记录）
        
        let result = sqlx::query(
            "INSERT INTO im_friendship_request 
             (id, from_id, to_id, remark, read_status, add_source, message, approve_status, 
              create_time, update_time, sequence, del_flag, version) 
             VALUES (?, ?, ?, ?, 0, ?, ?, 0, ?, ?, ?, 1, 1)
             ON DUPLICATE KEY UPDATE 
             approve_status = 0, update_time = ?, version = version + 1"
        )
        .bind(&request.id)
        .bind(&request.from_id)
        .bind(&request.to_id)
        .bind(&request.remark)
        .bind(&request.add_source)
        .bind(&request.message)
        .bind(now)  // create_time
        .bind(now)  // update_time
        .bind(now)  // sequence
        .bind(now)  // 用于 ON DUPLICATE KEY UPDATE 的 update_time
        .execute(&self.pool)
        .await;

        match result {
            Ok(_) => Ok(()),
            Err(e) => {
                error!("创建好友请求数据库错误: request_id={}, from_id={}, to_id={}, error={:?}", 
                      request.id, request.from_id, request.to_id, e);
                // 检查是否是外键约束错误
                if let sqlx::Error::Database(db_err) = &e {
                    let error_msg = db_err.message();
                    if error_msg.contains("foreign key constraint") || error_msg.contains("FOREIGN KEY") {
                        warn!("外键约束错误: 发送者 {} 或接收者 {} 可能不存在", request.from_id, request.to_id);
                        return Err(ErrorCode::NotFound);
                    }
                    if error_msg.contains("Duplicate entry") || error_msg.contains("PRIMARY") {
                        warn!("好友请求记录已存在: request_id={}, from_id={}, to_id={}", 
                             request.id, request.from_id, request.to_id);
                        // 对于重复键，ON DUPLICATE KEY UPDATE 应该已经处理，但如果还是失败，返回错误
                        return Err(ErrorCode::InvalidInput);
                    }
                    if error_msg.contains("Data too long") || error_msg.contains("too long") {
                        warn!("数据长度超过限制: request_id={}, from_id={}, to_id={}", 
                             request.id, request.from_id, request.to_id);
                        return Err(ErrorCode::InvalidInput);
                    }
                }
                Err(ErrorCode::Database)
            }
        }
    }

    /// 获取好友请求列表
    pub async fn get_friendship_requests(&self, to_id: &str, approve_status: Option<i32>) -> Result<Vec<ImFriendshipRequest>> {
        let mut query = "SELECT id, from_id, to_id, remark, read_status, add_source, message, 
                                approve_status, create_time, update_time, sequence, del_flag, version 
                         FROM im_friendship_request 
                         WHERE to_id = ? AND (del_flag IS NULL OR del_flag = 1)".to_string();

        if let Some(status) = approve_status {
            query.push_str(&format!(" AND approve_status = {}", status));
        }

        query.push_str(" ORDER BY create_time DESC");

        let requests = sqlx::query_as::<_, ImFriendshipRequest>(&query)
            .bind(to_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

        Ok(requests)
    }

    /// 处理好友请求（同意或拒绝）
    pub async fn handle_friendship_request(&self, request_id: &str, approve_status: i32) -> Result<()> {
        let now = now_timestamp();

        // 更新请求状态
        sqlx::query(
            "UPDATE im_friendship_request 
             SET approve_status = ?, update_time = ?, version = version + 1 
             WHERE id = ?"
        )
        .bind(approve_status)
        .bind(now)
        .bind(request_id)
        .execute(&self.pool)
        .await
        .map_err(|_| ErrorCode::Database)?;

        // 如果同意，添加好友关系
        if approve_status == 1 {
            let request = sqlx::query_as::<_, ImFriendshipRequest>(
                "SELECT id, from_id, to_id, remark, read_status, add_source, message, 
                        approve_status, create_time, update_time, sequence, del_flag, version 
                 FROM im_friendship_request 
                 WHERE id = ?"
            )
            .bind(request_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ErrorCode::Database)?;

            if let Some(req) = request {
                self.add_friend(req.from_id, req.to_id, req.add_source, req.remark).await?;
            }
        }

        Ok(())
    }
}

