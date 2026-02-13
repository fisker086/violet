//! WebSocket 握手鉴权：从 Query / Header / Cookie 提取 token，JWT 校验后得到 userId

use axum::{
    extract::Request,
    http::{header, StatusCode},
};
use jsonwebtoken::{decode, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Clone, Deserialize)]
pub struct JwtClaims {
    pub sub: Option<String>,
    /// 部分实现用 username 表示用户 ID
    pub username: Option<String>,
    #[allow(dead_code)]
    pub exp: Option<i64>,
    #[serde(flatten)]
    #[allow(dead_code)]
    pub rest: HashMap<String, serde_json::Value>,
}

/// 从请求中提取 token（Query token= / Authorization: Bearer / Cookie token=）
#[allow(dead_code)]
pub fn extract_token_from_request(req: &Request) -> Option<String> {
    // 1. Query: ?token=xxx
    if let Some(q) = req.uri().query() {
        for part in q.split('&') {
            let mut it = part.splitn(2, '=');
            if it.next().map(|k| k == "token").unwrap_or(false) {
                if let Some(v) = it.next() {
                    let v = v.trim();
                    if !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
            }
        }
    }

    // 2. Authorization: Bearer xxx
    if let Some(auth) = req.headers().get(header::AUTHORIZATION) {
        if let Ok(s) = auth.to_str() {
            let s = s.trim();
            if s.len() > 7 && s.get(0..7).map(|p| p.eq_ignore_ascii_case("bearer ")).unwrap_or(false) {
                let t = s[7..].trim();
                if !t.is_empty() {
                    return Some(t.to_string());
                }
            }
        }
    }

    // 3. Cookie: token=xxx
    if let Some(cookie) = req.headers().get(header::COOKIE) {
        if let Ok(s) = cookie.to_str() {
            for part in s.split(';') {
                let part = part.trim();
                if let Some(rest) = part.strip_prefix("token=") {
                    let v = rest.trim();
                    if !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
            }
        }
    }

    None
}

/// 校验 token，返回 userId（从 sub 或 username 取）
pub fn validate_token(secret: &str, token: &str) -> Result<String, jsonwebtoken::errors::Error> {
    let mut validation = Validation::default();
    validation.validate_exp = true;
    let key = DecodingKey::from_secret(secret.as_bytes());
    let data = decode::<JwtClaims>(token, &key, &validation)?;
    let user_id = data
        .claims
        .sub
        .or(data.claims.username)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| jsonwebtoken::errors::Error::from(jsonwebtoken::errors::ErrorKind::InvalidToken))?;
    Ok(user_id)
}

/// 从 URI 或 Header 取 deviceType
#[allow(dead_code)]
pub fn extract_device_type(req: &Request) -> Option<String> {
    if let Some(q) = req.uri().query() {
        for part in q.split('&') {
            let mut it = part.splitn(2, '=');
            if it.next().map(|k| k == "deviceType").unwrap_or(false) {
                if let Some(v) = it.next() {
                    let v = v.trim();
                    if !v.is_empty() {
                        return Some(v.to_string());
                    }
                }
            }
        }
    }
    req.headers()
        .get("X-Device-Type")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[allow(dead_code)]
pub fn unauthorized() -> (StatusCode, &'static str) {
    (StatusCode::UNAUTHORIZED, "Unauthorized")
}
