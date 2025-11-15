use jsonwebtoken::{encode, decode, Header, Algorithm, Validation, EncodingKey, DecodingKey};
use serde::{Deserialize, Serialize};
use chrono::{Utc, Duration};

/// JWT 配置
#[derive(Debug, Clone, Deserialize)]
pub struct JwtSettings {
    pub secret: String,
    #[serde(default = "default_expiration_hours")]
    pub expiration_hours: u64,
}

fn default_expiration_hours() -> u64 {
    24
}

/// JWT Claims
#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    // 使用 open_id 解析后的数字（如果是数字字符串），否则使用数据库 id（向后兼容）
    pub user_id: u64,
    // 标识这是 open_id 的数字形式还是数据库 id
    #[serde(default)]
    pub is_open_id: bool,
    pub exp: i64,
    pub iat: i64,
}

impl Claims {
    /// 创建新的 Claims，使用 open_id 解析后的数字
    /// open_id 必须是数字字符串（雪花算法生成的）
    pub fn new_with_open_id(open_id_number: u64, expiration_hours: u64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(expiration_hours as i64);
        
        Claims {
            user_id: open_id_number,
            is_open_id: true,
            exp: exp.timestamp(),
            iat: now.timestamp(),
        }
    }
    
    /// 创建新的 Claims，使用数据库 id（向后兼容旧 token）
    pub fn new_with_db_id(user_id: u64, expiration_hours: u64) -> Self {
        let now = Utc::now();
        let exp = now + Duration::hours(expiration_hours as i64);
        
        Claims {
            user_id,
            is_open_id: false,
            exp: exp.timestamp(),
            iat: now.timestamp(),
        }
    }
    
    /// 兼容旧版本：默认使用数据库 id
    #[deprecated(note = "使用 new_with_open_id 代替")]
    pub fn new(user_id: u64, expiration_hours: u64) -> Self {
        Self::new_with_db_id(user_id, expiration_hours)
    }
}

/// 生成 token，使用 open_id（解析为数字）
/// open_id 必须是数字字符串（雪花算法生成的）
pub fn generate_token_with_open_id(open_id_number: u64, jwt_cfg: &JwtSettings) -> anyhow::Result<String> {
    let claims = Claims::new_with_open_id(open_id_number, jwt_cfg.expiration_hours);
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_cfg.secret.as_ref()),
    )?;
    
    Ok(token)
}

/// 生成 token，使用数据库 id（向后兼容）
pub fn generate_token(user_id: u64, jwt_cfg: &JwtSettings) -> anyhow::Result<String> {
    let claims = Claims::new_with_db_id(user_id, jwt_cfg.expiration_hours);
    let token = encode(
        &Header::new(Algorithm::HS256),
        &claims,
        &EncodingKey::from_secret(jwt_cfg.secret.as_ref()),
    )?;
    
    Ok(token)
}

/// 验证 token
pub fn verify_token(token: &str, jwt_cfg: &JwtSettings) -> anyhow::Result<Claims> {
    let validation = Validation::new(Algorithm::HS256);
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(jwt_cfg.secret.as_ref()),
        &validation,
    )?;
    
    Ok(token_data.claims)
}

