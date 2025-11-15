use axum::{extract::{Extension, Query}, http::StatusCode, response::IntoResponse, Json};
use axum::body::Body;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::{
    error::{ErrorResponse, ErrorCode},
    config::SrsSettings,
};
use tracing::{error, info, warn};

#[derive(Deserialize)]
pub struct WebRtcTokenRequest {
    #[allow(dead_code)]
    pub stream: String,  // 流名称，通常是 user_id 或 room_id
    #[allow(dead_code)]
    pub publish: Option<bool>,  // 是否为推流，默认为 false（拉流）
}

#[derive(Serialize)]
pub struct WebRtcTokenResponse {
    pub code: i32,
    pub server: String,  // SRS WebRTC 服务器地址
    pub sdp: Option<String>,  // SDP 信息（如果 SRS API 返回）
    pub sessionid: Option<String>,  // 会话 ID
}

/// 获取 WebRTC token（通过 SRS API）
pub async fn get_webrtc_token(
    Extension(srs_config): Extension<SrsSettings>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let stream = params.get("stream")
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "缺少 stream 参数")),
        ))?;
    
    let publish = params.get("publish")
        .map(|s| s == "true" || s == "1")
        .unwrap_or(false);
    
    info!(stream = %stream, publish = %publish, "获取 WebRTC token");
    
    // 调用 SRS API 获取 WebRTC token
    // SRS WebRTC API 格式: http://server:port/rtc/v1/publish/ 或 /rtc/v1/play/
    let api_url = if publish {
        // 推流：/rtc/v1/publish/
        format!("{}/rtc/v1/publish/", srs_config.host)
    } else {
        // 拉流：/rtc/v1/play/
        format!("{}/rtc/v1/play/", srs_config.host)
    };
    
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!(error = %e, "创建 HTTP 客户端失败");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::Internal, format!("创建 HTTP 客户端失败: {}", e))),
            ));
        }
    };
    
    // 构建请求体
    let request_body = serde_json::json!({
        "api": format!("{}/rtc/v1/{}/", srs_config.host, if publish { "publish" } else { "play" }),
        "streamurl": format!("webrtc://{}/{}/{}", srs_config.host, srs_config.app, stream),
        "app": srs_config.app,
        "stream": stream,
    });
    
    info!(url = %api_url, body = %serde_json::to_string(&request_body).unwrap_or_default(), "发送 WebRTC token 请求");
    
    match client.post(&api_url)
        .json(&request_body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(text) => {
                    if status.is_success() {
                        // 尝试解析 JSON 响应
                        match serde_json::from_str::<serde_json::Value>(&text) {
                            Ok(json) => {
                                let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                                let server = json.get("server")
                                    .and_then(|v| v.as_str())
                                    .map(|s| s.to_string())
                                    .unwrap_or_else(|| srs_config.host.clone());
                                let sdp = json.get("sdp").and_then(|v| v.as_str()).map(|s| s.to_string());
                                let sessionid = json.get("sessionid").and_then(|v| v.as_str()).map(|s| s.to_string());
                                
                                Ok((
                                    StatusCode::OK,
                                    Json(WebRtcTokenResponse {
                                        code: code as i32,
                                        server,
                                        sdp,
                                        sessionid,
                                    }),
                                ))
                            },
                            Err(_) => Err((
                                StatusCode::BAD_GATEWAY,
                                Json(ErrorResponse::new(ErrorCode::Internal, "解析 SRS 响应失败")),
                            )),
                        }
                    } else {
                        error!(status = %status, response = %text, "SRS API 返回错误");
                        Err((
                            status,
                            Json(ErrorResponse::new(ErrorCode::Internal, format!("SRS API 返回错误: {}", text))),
                        ))
                    }
                },
                Err(e) => {
                    error!(error = %e, "读取响应失败");
                    Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse::new(ErrorCode::Internal, format!("读取响应失败: {}", e))),
                    ))
                }
            }
        }
        Err(e) => {
            error!(error = %e, "请求 SRS API 失败");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::with_details(ErrorCode::Internal, "连接 SRS 服务器失败", e.to_string())),
            ))
        }
    }
}

/// 获取 WebRTC 推流 token
pub async fn get_webrtc_publish_token(
    Extension(srs_config): Extension<SrsSettings>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let mut params = params;
    params.insert("publish".to_string(), "true".to_string());
    get_webrtc_token(Extension(srs_config), Query(params)).await
}

/// 获取 WebRTC 拉流 token
pub async fn get_webrtc_play_token(
    Extension(srs_config): Extension<SrsSettings>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    get_webrtc_token(Extension(srs_config), Query(params)).await
}

/// 代理 SRS WebRTC 推流 SDP 交换
pub async fn proxy_rtc_publish(
    Extension(srs_config): Extension<SrsSettings>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> impl IntoResponse {
    let api_url = format!("{}/rtc/v1/publish/", srs_config.host);
    info!(url = %api_url, "代理 SRS 推流请求");
    
    // 从请求体中提取必要信息
    let stream_name = body.get("stream")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "缺少 stream 参数")),
        ))?;
    
    let sdp = body.get("sdp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "缺少 sdp 参数")),
        ))?;
    
    let app_name = srs_config.app.clone();
    
    // 构建完整的 SRS 请求体
    let srs_request_body = serde_json::json!({
        "api": format!("{}/rtc/v1/publish/", srs_config.host),
        "streamurl": format!("webrtc://{}/{}/{}", srs_config.host, app_name, stream_name),
        "app": app_name,
        "stream": stream_name,
        "sdp": sdp,
    });
    
    info!(
        stream = %stream_name,
        app = %app_name,
        sdp_length = sdp.len(),
        "发送推流请求到 SRS"
    );
    
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!(error = %e, "创建 HTTP 客户端失败");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::Internal, format!("创建 HTTP 客户端失败: {}", e))),
            ));
        }
    };
    
    match client.post(&api_url)
        .json(&srs_request_body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(text) => {
                    if status.is_success() {
                        // SRS 可能返回 SDP 格式或 JSON 格式
                        if text.trim().starts_with("v=") {
                            // SDP 格式，直接返回
                            info!(sdp_length = text.len(), "收到 SDP 格式响应");
                            Ok((status, Body::from(text)).into_response())
                        } else {
                            // JSON 格式，解析并返回
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(json) => {
                                    let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                                    if code == 0 {
                                        if let Some(sdp) = json.get("sdp").and_then(|v| v.as_str()) {
                                            info!(sdp_length = sdp.len(), "从 JSON 响应中提取 SDP");
                                            Ok((status, Body::from(sdp.to_string())).into_response())
                                        } else {
                                            error!(response = %text, "JSON 响应中没有 sdp 字段");
                                            Err((
                                                StatusCode::BAD_GATEWAY,
                                                Json(ErrorResponse::new(ErrorCode::Internal, "SRS 响应格式错误：缺少 sdp 字段")),
                                            ))
                                        }
                                    } else {
                                        let message = json.get("message")
                                            .or_else(|| json.get("msg"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("未知错误");
                                        error!(code = %code, message = %message, "SRS API 返回错误代码");
                                        Err((
                                            StatusCode::BAD_GATEWAY,
                                            Json(ErrorResponse::new(ErrorCode::Internal, format!("SRS API 错误: {}", message))),
                                        ))
                                    }
                                },
                                Err(_) => Ok((status, Body::from(text)).into_response()),
                            }
                        }
                    } else {
                        error!(status = %status, response = %text, "SRS API 返回错误状态");
                        Err((
                            status,
                            Json(ErrorResponse::new(ErrorCode::Internal, format!("SRS API 返回错误: {}", text))),
                        ))
                    }
                },
                Err(e) => {
                    error!(error = %e, "读取响应失败");
                    Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse::new(ErrorCode::Internal, format!("读取响应失败: {}", e))),
                    ))
                }
            }
        }
        Err(e) => {
            error!(error = %e, "请求 SRS API 失败");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::with_details(ErrorCode::Internal, "连接 SRS 服务器失败", e.to_string())),
            ))
        }
    }
}

/// 检查流是否存在（通过 SRS API）
async fn check_stream_exists(
    srs_config: &SrsSettings,
    app: &str,
    stream: &str,
) -> Result<bool, String> {
    // SRS 6.0 使用 /api/v1/streams/ 查询所有流，然后过滤
    // 或者使用 /api/v1/sessions/ 查询会话
    // 先尝试查询所有流
    let api_url = format!("{}/api/v1/streams/", srs_config.host);
    
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(e) => return Err(format!("创建 HTTP 客户端失败: {}", e)),
    };
    
    match client.get(&api_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                // 解析响应，查找匹配的流
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        // SRS 返回格式可能是 {"streams": [...]} 或直接是数组
                        let streams = json.get("streams")
                            .and_then(|v| v.as_array())
                            .or_else(|| json.as_array());
                        
                        if let Some(streams_array) = streams {
                            for stream_obj in streams_array {
                                if let Some(stream_name) = stream_obj.get("name").and_then(|v| v.as_str()) {
                                    if stream_name == stream {
                                        if let Some(app_name) = stream_obj.get("app").and_then(|v| v.as_str()) {
                                            if app_name == app {
                                                return Ok(true);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Ok(false)
                    }
                    Err(e) => Err(format!("解析流列表失败: {}", e)),
                }
            } else {
                // API 返回非成功状态，尝试直接查询单个流
                let direct_url = format!("{}/api/v1/streams/{}/{}", srs_config.host, app, stream);
                match client.get(&direct_url).send().await {
                    Ok(resp) => Ok(resp.status().is_success()),
                    Err(e) => Err(format!("查询流状态失败: {}", e)),
                }
            }
        }
        Err(e) => Err(format!("查询流状态失败: {}", e)),
    }
}

/// 代理 SRS WebRTC 拉流 SDP 交换
pub async fn proxy_rtc_play(
    Extension(srs_config): Extension<SrsSettings>,
    axum::extract::Json(body): axum::extract::Json<serde_json::Value>,
) -> impl IntoResponse {
    let api_url = format!("{}/rtc/v1/play/", srs_config.host);
    info!(url = %api_url, "代理 SRS 拉流请求");
    
    // 从请求体中提取必要信息
    let stream_name = body.get("stream")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "缺少 stream 参数")),
        ))?;
    
    let sdp = body.get("sdp")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "缺少 sdp 参数")),
        ))?;
    
    let app_name = srs_config.app.clone();
    
    // 如果流名称可用，检查流是否存在，如果不存在则等待并重试
    let mut stream_exists = false;
    {
        // 最多重试 3 次，每次等待 500ms
        for attempt in 1..=3 {
            match check_stream_exists(&srs_config, &app_name, &stream_name).await {
                Ok(exists) => {
                    if exists {
                        info!(
                            stream = %stream_name,
                            app = %app_name,
                            attempt = %attempt,
                            "拉流请求：流已存在"
                        );
                        stream_exists = true;
                        break;
                    } else if attempt < 3 {
                        warn!(
                            stream = %stream_name,
                            app = %app_name,
                            attempt = %attempt,
                            "流尚未发布，等待 500ms 后重试"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    } else {
                        warn!(
                            stream = %stream_name,
                            app = %app_name,
                            stream_exists = %stream_exists,
                            "拉流请求：流尚未发布（已重试 3 次），SRS 可能返回错误"
                        );
                    }
                }
                Err(e) => {
                    if attempt < 3 {
                        warn!(
                            stream = %stream_name,
                            app = %app_name,
                            attempt = %attempt,
                            error = %e,
                            "检查流状态失败，等待 500ms 后重试"
                        );
                        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    } else {
                        warn!(
                            stream = %stream_name,
                            app = %app_name,
                            error = %e,
                            "检查流状态失败（已重试 3 次），继续尝试拉流"
                        );
                    }
                }
            }
        }
    }
    
    // 构建完整的 SRS 请求体
    let srs_request_body = serde_json::json!({
        "api": format!("{}/rtc/v1/play/", srs_config.host),
        "streamurl": format!("webrtc://{}/{}/{}", srs_config.host, app_name, stream_name),
        "app": app_name,
        "stream": stream_name,
        "sdp": sdp,
    });
    
    info!(
        stream = %stream_name,
        app = %app_name,
        sdp_length = sdp.len(),
        stream_exists = %stream_exists,
        "发送拉流请求到 SRS"
    );
    
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!(error = %e, "创建 HTTP 客户端失败");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::Internal, format!("创建 HTTP 客户端失败: {}", e))),
            ));
        }
    };
    
    match client.post(&api_url)
        .json(&srs_request_body)
        .send()
        .await
    {
        Ok(resp) => {
            let status = resp.status();
            match resp.text().await {
                Ok(text) => {
                    if status.is_success() {
                        // SRS 可能返回 SDP 格式或 JSON 格式
                        if text.trim().starts_with("v=") {
                            // SDP 格式，直接返回
                            info!(sdp_length = text.len(), "收到 SDP 格式响应");
                            Ok((status, Body::from(text)).into_response())
                        } else {
                            // JSON 格式，解析并返回
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(json) => {
                                    let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(0);
                                    if code == 0 {
                                        if let Some(sdp) = json.get("sdp").and_then(|v| v.as_str()) {
                                            info!(sdp_length = sdp.len(), "从 JSON 响应中提取 SDP");
                                            Ok((status, Body::from(sdp.to_string())).into_response())
                                        } else {
                                            error!(response = %text, "JSON 响应中没有 sdp 字段");
                                            Err((
                                                StatusCode::BAD_GATEWAY,
                                                Json(ErrorResponse::new(ErrorCode::Internal, "SRS 响应格式错误：缺少 sdp 字段")),
                                            ))
                                        }
                                    } else {
                                        let message = json.get("message")
                                            .or_else(|| json.get("msg"))
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("未知错误");
                                        error!(code = %code, message = %message, "SRS API 返回错误代码");
                                        Err((
                                            StatusCode::BAD_GATEWAY,
                                            Json(ErrorResponse::new(ErrorCode::Internal, format!("SRS API 错误: {}", message))),
                                        ))
                                    }
                                },
                                Err(_) => Ok((status, Body::from(text)).into_response()),
                            }
                        }
                    } else {
                        error!(status = %status, response = %text, "SRS API 返回错误状态");
                        Err((
                            status,
                            Json(ErrorResponse::new(ErrorCode::Internal, format!("SRS API 返回错误: {}", text))),
                        ))
                    }
                },
                Err(e) => {
                    error!(error = %e, "读取响应失败");
                    Err((
                        StatusCode::BAD_GATEWAY,
                        Json(ErrorResponse::new(ErrorCode::Internal, format!("读取响应失败: {}", e))),
                    ))
                }
            }
        }
        Err(e) => {
            error!(error = %e, "请求 SRS API 失败");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::with_details(ErrorCode::Internal, "连接 SRS 服务器失败", e.to_string())),
            ))
        }
    }
}

/// 查询流的播放者数量（用于判断对方是否接听）
pub async fn check_stream_players(
    Extension(srs_config): Extension<SrsSettings>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let stream_name = params.get("stream")
        .ok_or_else(|| (
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(ErrorCode::InvalidInput, "缺少 stream 参数")),
        ))?;
    
    let app_name = srs_config.app.clone();
    
    info!(stream = %stream_name, "查询流的播放者数量");
    
    // 通过 SRS API 查询会话信息
    // SRS 6.0 使用 /api/v1/sessions/ 查询所有会话
    let api_url = format!("{}/api/v1/sessions/", srs_config.host);
    
    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
    {
        Ok(client) => client,
        Err(e) => {
            error!(error = %e, "创建 HTTP 客户端失败");
            return Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(ErrorCode::Internal, format!("创建 HTTP 客户端失败: {}", e))),
            ));
        }
    };
    
    match client.get(&api_url).send().await {
        Ok(resp) => {
            if resp.status().is_success() {
                match resp.json::<serde_json::Value>().await {
                    Ok(json) => {
                        // SRS 返回格式可能是 {"sessions": [...]} 或直接是数组
                        let sessions = json.get("sessions")
                            .and_then(|v| v.as_array())
                            .or_else(|| json.as_array());
                        
                        let mut player_count = 0;
                        
                        if let Some(sessions_array) = sessions {
                            for session_obj in sessions_array {
                                if let Some(obj) = session_obj.as_object() {
                                    // 检查是否是播放会话（type 为 "play" 或 "rtc-play"）
                                    let session_type = obj.get("type")
                                        .and_then(|v| v.as_str())
                                        .unwrap_or("");
                                    
                                    // 检查流名称是否匹配
                                    let session_stream = obj.get("stream")
                                        .and_then(|v| v.as_str())
                                        .or_else(|| {
                                            // 有些版本可能使用 name 字段
                                            obj.get("name").and_then(|v| v.as_str())
                                        });
                                    
                                    // 检查 app 是否匹配
                                    let session_app = obj.get("app")
                                        .and_then(|v| v.as_str());
                                    
                                    // 判断是否是播放会话（拉流）
                                    let is_play_session = session_type == "play" || 
                                                         session_type == "rtc-play" ||
                                                         session_type == "client" ||
                                                         (session_type.is_empty() && 
                                                          obj.get("publish").and_then(|v| v.as_bool()) == Some(false));
                                    
                                    if is_play_session {
                                        if let Some(sess_stream) = session_stream {
                                            if sess_stream == stream_name {
                                                if let Some(sess_app) = session_app {
                                                    if sess_app == app_name {
                                                        player_count += 1;
                                                        info!(
                                                            stream = %stream_name,
                                                            session_id = ?obj.get("id"),
                                                            "找到播放会话"
                                                        );
                                                    }
                                                } else {
                                                    // 如果没有 app 字段，也认为匹配（兼容性）
                                                    player_count += 1;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        info!(
                            stream = %stream_name,
                            player_count = %player_count,
                            "查询播放者数量完成"
                        );
                        
                        Ok((
                            StatusCode::OK,
                            Json(serde_json::json!({
                                "code": 0,
                                "message": "查询成功",
                                "data": {
                                    "stream": stream_name,
                                    "player_count": player_count,
                                    "has_players": player_count > 0
                                }
                            })),
                        ))
                    }
                    Err(e) => {
                        error!(error = %e, "解析 SRS 响应失败");
                        Err((
                            StatusCode::BAD_GATEWAY,
                            Json(ErrorResponse::new(ErrorCode::Internal, format!("解析 SRS 响应失败: {}", e))),
                        ))
                    }
                }
            } else {
                error!(status = %resp.status(), "SRS API 返回错误状态");
                Err((
                    StatusCode::BAD_GATEWAY,
                    Json(ErrorResponse::new(ErrorCode::Internal, format!("SRS API 返回错误: {}", resp.status()))),
                ))
            }
        }
        Err(e) => {
            error!(error = %e, "请求 SRS API 失败");
            Err((
                StatusCode::BAD_GATEWAY,
                Json(ErrorResponse::with_details(ErrorCode::Internal, "连接 SRS 服务器失败", e.to_string())),
            ))
        }
    }
}
