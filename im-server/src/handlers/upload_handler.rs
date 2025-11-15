use axum::{
    extract::{Extension, Multipart},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    body::Body,
};
use sqlx::MySqlPool;
use std::path::PathBuf;
use tracing::{error, info, warn};
use uuid::Uuid;
use crate::error::{ErrorCode, ErrorResponse};
use crate::config::UploadSettings;
use crate::middleware::auth::UserIdentity;
use crate::service::{ImFriendshipService, ImGroupService};
use image::{ImageFormat, imageops::FilterType};
use std::io::Cursor;

/// 初始化上传目录
pub fn init_upload_dir(upload_path: &str) -> std::io::Result<()> {
    std::fs::create_dir_all(upload_path)?;
    Ok(())
}

/// 处理图片：压缩和生成缩略图
async fn process_image(
    image_data: &[u8],
    upload_settings: &UploadSettings,
    unique_file_name: &str,
    upload_dir: &PathBuf,
    open_id: &str,
) -> Result<(Option<String>, Option<String>), String> {
    // 解码图片
    let img = match image::load_from_memory(image_data) {
        Ok(img) => img,
        Err(e) => {
            return Err(format!("无法解码图片: {}", e));
        }
    };

    let original_width = img.width();
    let original_height = img.height();
    
    // 计算缩略图尺寸（保持宽高比）
    let (thumb_width, thumb_height) = if original_width > upload_settings.thumbnail_max_width 
        || original_height > upload_settings.thumbnail_max_height {
        let ratio = (upload_settings.thumbnail_max_width as f32 / original_width as f32)
            .min(upload_settings.thumbnail_max_height as f32 / original_height as f32);
        (
            (original_width as f32 * ratio) as u32,
            (original_height as f32 * ratio) as u32,
        )
    } else {
        (original_width, original_height)
    };

    // 生成缩略图（使用 resize 保持宽高比，而不是 resize_exact）
    let thumbnail = img.resize(thumb_width, thumb_height, FilterType::Lanczos3);
    
    // 确定图片格式
    let format = match PathBuf::from(unique_file_name).extension()
        .and_then(|ext| ext.to_str()) {
        Some("jpg") | Some("jpeg") => ImageFormat::Jpeg,
        Some("png") => ImageFormat::Png,
        Some("webp") => ImageFormat::WebP,
        Some("gif") => ImageFormat::Gif,
        _ => ImageFormat::Jpeg, // 默认使用 JPEG
    };

    let mut original_path = None;
    let thumbnail_path;

    // 保存原图（如果需要）
    if upload_settings.save_original {
        let original_file_path = upload_dir.join(open_id).join(unique_file_name);
        let mut original_buffer = Vec::new();
        
        match format {
            ImageFormat::Jpeg => {
                let mut cursor = Cursor::new(&mut original_buffer);
                if let Err(e) = img.write_to(&mut cursor, ImageFormat::Jpeg) {
                    return Err(format!("保存原图失败: {}", e));
                }
            }
            ImageFormat::Png => {
                let mut cursor = Cursor::new(&mut original_buffer);
                if let Err(e) = img.write_to(&mut cursor, ImageFormat::Png) {
                    return Err(format!("保存原图失败: {}", e));
                }
            }
            ImageFormat::WebP => {
                // WebP 需要特殊处理，这里先转换为 JPEG
                let mut cursor = Cursor::new(&mut original_buffer);
                if let Err(e) = img.write_to(&mut cursor, ImageFormat::Jpeg) {
                    return Err(format!("保存原图失败: {}", e));
                }
            }
            ImageFormat::Gif => {
                // GIF 保持原样（GIF 动画需要特殊处理，这里简化处理）
                // 如果原图是 GIF 且需要处理，可以转换为静态图片
                // 这里为了保持兼容性，直接保存原数据
                original_buffer = image_data.to_vec();
            }
            _ => {
                // 其他格式转换为 JPEG
                let mut cursor = Cursor::new(&mut original_buffer);
                if let Err(e) = img.write_to(&mut cursor, ImageFormat::Jpeg) {
                    return Err(format!("保存原图失败: {}", e));
                }
            }
        }

        if let Err(e) = tokio::fs::write(&original_file_path, &original_buffer).await {
            return Err(format!("写入原图文件失败: {}", e));
        }
        original_path = Some(format!("{}/{}", open_id, unique_file_name));
    }

    // 保存缩略图
    let thumb_file_name = format!("thumb_{}", unique_file_name);
    let thumb_file_path = upload_dir.join(open_id).join(&thumb_file_name);
    let mut thumb_buffer = Vec::new();
    let mut cursor = Cursor::new(&mut thumb_buffer);
    
    // 缩略图统一使用 JPEG 格式，质量可配置
    if let Err(e) = thumbnail.write_to(&mut cursor, ImageFormat::Jpeg) {
        return Err(format!("生成缩略图失败: {}", e));
    }

    // 如果启用了压缩，对缩略图进行进一步压缩
    // 注意：image crate 的 write_to 不直接支持质量参数
    // 这里我们使用 resize 已经减少了尺寸，如果需要更激进的压缩，可以考虑使用其他库
    // 但通常 resize 到合适尺寸已经能显著减少文件大小

    if let Err(e) = tokio::fs::write(&thumb_file_path, &thumb_buffer).await {
        return Err(format!("写入缩略图文件失败: {}", e));
    }
    thumbnail_path = Some(format!("{}/{}", open_id, thumb_file_name));

    Ok((original_path, thumbnail_path))
}

/// 上传文件
pub async fn upload_file(
    Extension(_pool): Extension<MySqlPool>,
    Extension(upload_settings): Extension<UploadSettings>,
    Extension(user_identity): Extension<UserIdentity>,
    mut multipart: Multipart,
) -> impl IntoResponse {
    let open_id = &user_identity.open_id;
    
    // 确保用户的上传目录存在
    let user_upload_dir = PathBuf::from(&upload_settings.path).join(open_id);
    if let Err(e) = init_upload_dir(user_upload_dir.to_str().unwrap()) {
        error!(error = %e, open_id = %open_id, "创建用户上传目录失败");
        return Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(ErrorResponse::new(
                ErrorCode::Internal,
                "创建上传目录失败",
            )),
        ));
    }

    // 从 multipart 中读取文件
    while let Some(field) = multipart.next_field().await.unwrap_or(None) {
        let field_name = field.name().unwrap_or("unknown");
        
        if field_name == "file" {
            let file_name = field.file_name()
                .unwrap_or("unknown")
                .to_string();
            
            // 在调用 bytes() 之前先获取 content_type，因为 bytes() 会移动 field
            let content_type = field.content_type()
                .unwrap_or("")
                .to_string();
            
            let file_data = match field.bytes().await {
                Ok(data) => data,
                Err(e) => {
                    error!(error = %e, "读取文件数据失败");
                    return Err((
                        StatusCode::BAD_REQUEST,
                        Json(ErrorResponse::new(
                            ErrorCode::InvalidInput,
                            "读取文件数据失败",
                        )),
                    ));
                }
            };

            // 根据文件类型验证文件大小
            let is_image = content_type.starts_with("image/");
            let (max_size_mb, max_size_bytes) = if is_image {
                (upload_settings.max_image_size_mb, upload_settings.max_image_size_mb * 1024 * 1024)
            } else {
                (upload_settings.max_file_size_mb, upload_settings.max_file_size_mb * 1024 * 1024)
            };

            if file_data.len() > max_size_bytes as usize {
                let file_type = if is_image { "图片" } else { "文件" };
                return Err((
                    StatusCode::PAYLOAD_TOO_LARGE,
                    Json(ErrorResponse::new(
                        ErrorCode::InvalidInput,
                        format!("{}大小不能超过 {}MB", file_type, max_size_mb),
                    )),
                ));
            }

            // 生成唯一文件名：UUID + 原始扩展名
            let file_path_buf = PathBuf::from(&file_name);
            let extension = file_path_buf
                .extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or(if is_image { "jpg" } else { "bin" });
            
            let unique_file_name = format!("{}.{}", Uuid::new_v4(), extension);
            let upload_dir = PathBuf::from(&upload_settings.path);

            // 如果是图片文件，进行图片处理（压缩和生成缩略图）
            if is_image && upload_settings.enable_image_processing {
                match process_image(
                    &file_data,
                    &upload_settings,
                    &unique_file_name,
                    &upload_dir,
                    open_id,
                ).await {
                    Ok((original_path, thumbnail_path)) => {
                        info!(
                            file_name = %unique_file_name,
                            original = ?original_path,
                            thumbnail = ?thumbnail_path,
                            "图片处理成功"
                        );

                        // 返回文件信息（优先返回缩略图 URL，如果存在）
                        // URL 中包含 open_id 路径，用于权限验证
                        let display_url = thumbnail_path
                            .as_ref()
                            .map(|t| format!("/api/upload/{}", t))
                            .or_else(|| original_path.as_ref().map(|o| format!("/api/upload/{}", o)))
                            .unwrap_or_else(|| format!("/api/upload/{}/{}", open_id, unique_file_name));

                        return Ok((
                            StatusCode::OK,
                            Json(serde_json::json!({
                                "url": display_url,
                                "original_url": original_path.as_ref().map(|o| format!("/api/upload/{}", o)),
                                "thumbnail_url": thumbnail_path.as_ref().map(|t| format!("/api/upload/{}", t)),
                                "file_name": unique_file_name,
                                "file_type": content_type,
                            })),
                        ));
                    }
                    Err(e) => {
                        warn!(error = %e, "图片处理失败，将保存原图");
                        // 如果处理失败，降级为保存原图
                    }
                }
            }

            // 对于非图片文件，或图片处理未启用/失败的情况，直接保存原文件
            let file_path = upload_dir.join(open_id).join(&unique_file_name);
            if let Err(e) = tokio::fs::write(&file_path, &file_data).await {
                error!(file_name = %unique_file_name, error = %e, "保存文件失败");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(
                        ErrorCode::Internal,
                        "保存文件失败",
                    )),
                ));
            }

            info!(
                file_name = %unique_file_name, 
                open_id = %open_id, 
                size = file_data.len(), 
                file_type = %content_type,
                "文件上传成功"
            );

            // 返回文件信息（URL 中包含 open_id 路径）
            return Ok((
                StatusCode::OK,
                Json(serde_json::json!({
                    "url": format!("/api/upload/{}/{}", open_id, unique_file_name),
                    "file_name": unique_file_name,
                    "file_type": content_type,
                })),
            ));
        }
    }

    Err((
        StatusCode::BAD_REQUEST,
        Json(ErrorResponse::new(
            ErrorCode::InvalidInput,
            "未找到文件",
        )),
    ))
}

/// 获取文件
/// 路由使用 /upload/*path 来匹配包含 / 的路径（如 open_id/file_name）
pub async fn get_file(
    Extension(upload_settings): Extension<UploadSettings>,
    Extension(user_identity): Extension<UserIdentity>,
    Extension(pool): Extension<MySqlPool>,
    axum::extract::Path(file_path_param): axum::extract::Path<String>,
) -> impl IntoResponse {
    let current_open_id = &user_identity.open_id;
    
    // 解析文件路径：格式为 {open_id}/{file_name} 或 {open_id}/thumb_{file_name}
    // 注意：使用 /*path 时，路径参数可能包含前导斜杠，需要去掉
    let file_path_param = file_path_param.trim_start_matches('/');
    
    // 安全检查：防止路径遍历攻击
    let sanitized_path = file_path_param
        .replace("..", "")
        .replace("\\", "");
    
    // 分割路径，第一部分应该是 open_id
    let path_parts: Vec<&str> = sanitized_path.split('/').collect();
    
    if path_parts.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                ErrorCode::InvalidInput,
                "无效的文件路径",
            )),
        ));
    }
    
    // 处理旧格式文件（直接在根目录下，没有 open_id 前缀）
    if path_parts.len() < 2 {
        // 旧格式：文件直接在根目录下
        // 允许已认证用户访问（已经通过了认证中间件）
        let file_name = sanitized_path;
        
        // 构建文件路径（直接在根目录下）
        let file_path = PathBuf::from(&upload_settings.path).join(&file_name);
        
        // 检查文件是否存在
        if !file_path.exists() {
            return Err((
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new(
                    ErrorCode::NotFound,
                    "文件不存在",
                )),
            ));
        }
        
        // 读取并返回文件（旧格式文件允许已认证用户访问）
        match tokio::fs::read(&file_path).await {
            Ok(file_data) => {
                // 根据文件扩展名确定 Content-Type
                let content_type = match file_path.extension()
                    .and_then(|ext| ext.to_str()) {
                    Some("jpg") | Some("jpeg") => "image/jpeg",
                    Some("png") => "image/png",
                    Some("gif") => "image/gif",
                    Some("webp") => "image/webp",
                    _ => "application/octet-stream",
                };

                let response = Response::builder()
                    .status(StatusCode::OK)
                    .header("Content-Type", content_type)
                    .header("Cache-Control", "public, max-age=31536000") // 缓存一年
                    .body(Body::from(file_data))
                    .unwrap();
                
                return Ok(response);
            }
            Err(e) => {
                error!(file_name = %file_name, error = %e, "读取文件失败");
                return Err((
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(ErrorResponse::new(
                        ErrorCode::Internal,
                        "读取文件失败",
                    )),
                ));
            }
        }
    }
    
    // 新格式：{open_id}/{file_name}
    let file_owner_open_id = path_parts[0];
    let file_name = path_parts[1..].join("/");
    
    if file_name.is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new(
                ErrorCode::InvalidInput,
                "无效的文件路径",
            )),
        ));
    }
    
    // 权限检查：允许文件所有者、好友或同群组成员访问
    if file_owner_open_id != current_open_id {
        let friendship_service = ImFriendshipService::new(pool.clone());
        let group_service = ImGroupService::new(pool.clone());
        
        // 先检查是否是好友关系
        let is_friend = match friendship_service.is_friend(current_open_id, file_owner_open_id).await {
            Ok(is_friend) => is_friend,
            Err(e) => {
                warn!(
                    current_open_id = %current_open_id,
                    file_owner_open_id = %file_owner_open_id,
                    error = ?e,
                    "检查好友关系失败"
                );
                false
            }
        };
        
        // 如果不是好友，检查是否在同一个群组中
        if !is_friend {
            // 获取当前用户的所有群组
            let current_user_groups = match group_service.get_user_groups(current_open_id).await {
                Ok(groups) => groups,
                Err(e) => {
                    warn!(
                        current_open_id = %current_open_id,
                        error = ?e,
                        "获取当前用户群组失败"
                    );
                    vec![]
                }
            };
            
            // 获取文件所有者的所有群组
            let owner_groups = match group_service.get_user_groups(file_owner_open_id).await {
                Ok(groups) => groups,
                Err(e) => {
                    warn!(
                        file_owner_open_id = %file_owner_open_id,
                        error = ?e,
                        "获取文件所有者群组失败"
                    );
                    vec![]
                }
            };
            
            // 检查是否有共同的群组
            let has_common_group = current_user_groups.iter().any(|current_group| {
                owner_groups.iter().any(|owner_group| {
                    current_group.group_id == owner_group.group_id
                })
            });
            
            if !has_common_group {
                warn!(
                    current_open_id = %current_open_id,
                    file_owner_open_id = %file_owner_open_id,
                    "用户尝试访问非好友且非同群组的文件"
                );
                return Err((
                    StatusCode::FORBIDDEN,
                    Json(ErrorResponse::new(
                        ErrorCode::Forbidden,
                        "无权访问此文件",
                    )),
                ));
            }
        }
    }
    
    // 构建文件路径
    let file_path = PathBuf::from(&upload_settings.path)
        .join(file_owner_open_id)
        .join(&file_name);
    
    // 检查文件是否存在
    if !file_path.exists() {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new(
                ErrorCode::NotFound,
                "文件不存在",
            )),
        ));
    }

    // 读取文件
    match tokio::fs::read(&file_path).await {
        Ok(file_data) => {
            // 根据文件扩展名确定 Content-Type
            let content_type = match file_path.extension()
                .and_then(|ext| ext.to_str()) {
                Some("jpg") | Some("jpeg") => "image/jpeg",
                Some("png") => "image/png",
                Some("gif") => "image/gif",
                Some("webp") => "image/webp",
                _ => "application/octet-stream",
            };

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", content_type)
                .header("Cache-Control", "public, max-age=31536000") // 缓存一年
                .body(Body::from(file_data))
                .unwrap();
            
            Ok(response)
        }
        Err(e) => {
            error!(file_name = %file_name, error = %e, "读取文件失败");
            Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new(
                    ErrorCode::Internal,
                    "读取文件失败",
                )),
            ))
        }
    }
}
