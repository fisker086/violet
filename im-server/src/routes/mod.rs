use axum::{Router, middleware, Extension};
use sqlx::MySqlPool;
use std::sync::Arc;
use crate::{
    handlers::{
        user_handler, auth_handler, message_handler, subscription_handler, friend_handler,
        im_user_handler, im_friendship_handler, im_message_handler, im_chat_handler, im_group_handler,
        im_outbox_handler, upload_handler, webrtc_handler,
    },
    middleware::auth::auth_middleware,
    mqtt::MqttPublisher,
    config::{UploadSettings, SrsSettings},
    service::SubscriptionService,
    redis::RedisClient,
};
use im_share::JwtSettings;

/// 路由信息结构
#[derive(Debug, Clone)]
pub struct RouteInfo {
    pub method: String,
    pub path: String,
    pub auth_required: bool,
}

/// 获取所有路由信息
pub fn get_all_routes() -> Vec<RouteInfo> {
    let mut routes = Vec::new();
    
    // 公开路由（不需要认证）
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/auth/login".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/auth/register".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/users".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/subscriptions/{subscription_id}/user".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/users/check-name/{name}".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/users/{id}/name".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/users/{id}/snowflake_id".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/friendships/{open_id}/friends".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/users".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/auth/login".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/users/{user_id}".to_string(),
        auth_required: false,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/upload/{*path}".to_string(),
        auth_required: true,
    });
    
    // 受保护的路由（需要认证）
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/users/me".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/users/me".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/users/{id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/messages".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/messages".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/friends".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/friends/{friend_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "DELETE".to_string(),
        path: "/api/friends/{friend_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/users/{user_id}/data".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/users/{user_id}/data".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/friends".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/friends".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "DELETE".to_string(),
        path: "/api/im/friends/{to_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/friends/{to_id}/remark".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/friends/{to_id}/black".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/friendship-requests".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/friendship-requests".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/friendship-requests/{request_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/friends/debug".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/messages/single".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/messages/single".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/messages/single/{message_id}/read".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/messages/group".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/messages/group/{group_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/messages/group/{group_id}/{message_id}/read".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/messages/group/{group_id}/{message_id}/status".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/messages/group/{group_id}/status".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/chats".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/chats".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/chats/unread-stats".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/chats/{chat_id}/read-sequence".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/chats/{chat_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "DELETE".to_string(),
        path: "/api/im/chats/{chat_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/groups".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/groups".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/groups/{group_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/groups/{group_id}/members".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/groups/{group_id}/members/{member_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "DELETE".to_string(),
        path: "/api/im/groups/{group_id}/members/{member_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/groups/{group_id}/members/{member_id}/role".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "DELETE".to_string(),
        path: "/api/im/groups/{group_id}/dissolve".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "DELETE".to_string(),
        path: "/api/im/groups/{group_id}/delete".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/groups/{group_id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/outbox".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/outbox/pending".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/outbox/failed".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/im/outbox/{id}".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "PUT".to_string(),
        path: "/api/im/outbox/{id}/status".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/im/outbox/{id}/sent".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/upload".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/webrtc/token".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/webrtc/publish".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "GET".to_string(),
        path: "/api/webrtc/play".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/webrtc/rtc/v1/publish".to_string(),
        auth_required: true,
    });
    routes.push(RouteInfo {
        method: "POST".to_string(),
        path: "/api/webrtc/rtc/v1/play".to_string(),
        auth_required: true,
    });
    
    routes
}

/// 打印所有路由信息
pub fn print_routes() {
    let routes = get_all_routes();
    
    println!("\n╔══════════════════════════════════════════════════════════════════════════════╗");
    println!("║                           Registered Routes                                   ║");
    println!("╠══════════════════════════════════════════════════════════════════════════════╣");
    
    // 按是否需要认证分组打印
    let public_routes: Vec<_> = routes.iter().filter(|r| !r.auth_required).collect();
    let protected_routes: Vec<_> = routes.iter().filter(|r| r.auth_required).collect();
    
    if !public_routes.is_empty() {
        println!("║ Public Routes (No Authentication Required):                                ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        for route in &public_routes {
            let path = if route.path.len() > 65 {
                format!("{}...", &route.path[..62])
            } else {
                route.path.clone()
            };
            println!("║ {:<8} {:<65} ║", route.method, path);
        }
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
    }
    
    if !protected_routes.is_empty() {
        println!("║ Protected Routes (Authentication Required):                                 ║");
        println!("╠══════════════════════════════════════════════════════════════════════════════╣");
        for route in &protected_routes {
            let path = if route.path.len() > 65 {
                format!("{}...", &route.path[..62])
            } else {
                route.path.clone()
            };
            println!("║ {:<8} {:<65} ║", route.method, path);
        }
    }
    
    println!("╠══════════════════════════════════════════════════════════════════════════════╣");
    let total_text = format!("Total: {} routes ({} public, {} protected)", 
                            routes.len(), 
                            public_routes.len(), 
                            protected_routes.len());
    println!("║ {:<76} ║", total_text);
    println!("╚══════════════════════════════════════════════════════════════════════════════╝\n");
}

pub fn create_public_routes(
    pool: MySqlPool, 
    jwt_cfg: JwtSettings,
    upload_settings: UploadSettings,
    subscription_service: Arc<SubscriptionService>,
    redis_client: Arc<RedisClient>,
) -> Router {
    Router::new()
        .route("/auth/login", axum::routing::post(auth_handler::login))
        .route("/auth/register", axum::routing::post(user_handler::create_user))
        .route("/users", axum::routing::post(user_handler::create_user)) // 兼容前端
        .route("/subscriptions/{subscription_id}/user", axum::routing::get(subscription_handler::get_user_id_by_subscription))
        // 公开API：检查昵称是否可用（不需要认证）
        .route("/users/check-name/{name}", axum::routing::get(user_handler::check_name_available))
        // 内部服务API：根据用户ID获取用户名（不需要认证）
        .route("/users/{id}/name", axum::routing::get(user_handler::get_user_name))
        // 内部服务API：根据用户ID获取雪花ID（不需要认证）
        .route("/users/{id}/snowflake_id", axum::routing::get(user_handler::get_user_snowflake_id))
        // 内部服务API：根据 open_id 获取好友列表（不需要认证）
        .route("/im/friendships/{open_id}/friends", axum::routing::get(im_friendship_handler::get_friends_by_open_id))
        // IM 用户相关路由
        .route("/im/users", axum::routing::post(im_user_handler::create_user))
        .route("/im/auth/login", axum::routing::post(im_user_handler::login))
        .route("/im/users/{user_id}", axum::routing::get(im_user_handler::get_user))
        .layer(Extension(pool))
        .layer(Extension(jwt_cfg))
        .layer(Extension(upload_settings))
        .layer(Extension(redis_client))
        .with_state(subscription_service)
}

pub fn create_protected_routes(
    pool: MySqlPool,
    jwt_cfg: JwtSettings,
    upload_settings: UploadSettings,
    srs_settings: SrsSettings,
    publisher: MqttPublisher,
    subscription_service: Arc<SubscriptionService>,
    redis_client: Arc<RedisClient>,
) -> Router {
    Router::new()
        // 原有路由
        .route("/users/me", axum::routing::get(user_handler::get_current_user))
        .route("/users/me", axum::routing::put(user_handler::update_current_user))
        .route("/users/{id}", axum::routing::get(user_handler::get_user))
        .route("/messages", axum::routing::post(message_handler::send_message))
        .route("/messages", axum::routing::get(message_handler::get_messages))
        .route("/friends", axum::routing::get(friend_handler::get_friends))
        .route("/friends/{friend_id}", axum::routing::post(friend_handler::add_friend))
        .route("/friends/{friend_id}", axum::routing::delete(friend_handler::remove_friend))
        // IM 用户相关路由
        .route("/im/users/{user_id}/data", axum::routing::get(im_user_handler::get_user_data))
        .route("/im/users/{user_id}/data", axum::routing::put(im_user_handler::upsert_user_data))
        // IM 好友相关路由
        .route("/im/friends", axum::routing::get(im_friendship_handler::get_friends))
        .route("/im/friends", axum::routing::post(im_friendship_handler::add_friend))
        .route("/im/friends/{to_id}", axum::routing::delete(im_friendship_handler::remove_friend))
        .route("/im/friends/{to_id}/remark", axum::routing::put(im_friendship_handler::update_remark))
        .route("/im/friends/{to_id}/black", axum::routing::post(im_friendship_handler::black_friend))
        .route("/im/friendship-requests", axum::routing::get(im_friendship_handler::get_friendship_requests))
        .route("/im/friendship-requests", axum::routing::post(im_friendship_handler::create_friendship_request))
        .route("/im/friendship-requests/{request_id}", axum::routing::post(im_friendship_handler::handle_friendship_request))
        // 调试接口：查看好友关系数据
        .route("/im/friends/debug", axum::routing::get(im_friendship_handler::debug_friendship_data))
        // IM 消息相关路由
        .route("/im/messages/single", axum::routing::post(im_message_handler::send_single_message))
        .route("/im/messages/single", axum::routing::get(im_message_handler::get_single_messages))
        .route("/im/messages/single/{message_id}/read", axum::routing::post(im_message_handler::mark_single_message_read))
        .route("/im/messages/group", axum::routing::post(im_message_handler::send_group_message))
        .route("/im/messages/group/{group_id}", axum::routing::get(im_message_handler::get_group_messages))
        .route("/im/messages/group/{group_id}/{message_id}/read", axum::routing::post(im_message_handler::mark_group_message_read))
        .route("/im/messages/group/{group_id}/{message_id}/status", axum::routing::get(im_message_handler::get_group_message_status))
        .route("/im/messages/group/{group_id}/status", axum::routing::get(im_message_handler::get_user_group_message_status))
        // IM 聊天会话相关路由
        .route("/im/chats", axum::routing::get(im_chat_handler::get_user_chats))
        .route("/im/chats", axum::routing::post(im_chat_handler::get_or_create_chat))
        .route("/im/chats/unread-stats", axum::routing::get(im_chat_handler::get_unread_stats))
        .route("/im/chats/{chat_id}/read-sequence", axum::routing::put(im_chat_handler::update_read_sequence))
        .route("/im/chats/{chat_id}/remark", axum::routing::put(im_chat_handler::update_chat_remark))
        .route("/im/chats/{chat_id}", axum::routing::put(im_chat_handler::update_chat))
        .route("/im/chats/{chat_id}", axum::routing::delete(im_chat_handler::delete_chat))
        // IM 群组相关路由
        .route("/im/groups", axum::routing::get(im_group_handler::get_user_groups))
        .route("/im/groups", axum::routing::post(im_group_handler::create_group))
        .route("/im/groups/{group_id}", axum::routing::get(im_group_handler::get_group))
        .route("/im/groups/{group_id}/members", axum::routing::get(im_group_handler::get_group_members))
        .route("/im/groups/{group_id}/members/{member_id}", axum::routing::post(im_group_handler::add_group_member))
        .route("/im/groups/{group_id}/members/{member_id}", axum::routing::delete(im_group_handler::remove_group_member))
        .route("/im/groups/{group_id}/members/{member_id}/role", axum::routing::put(im_group_handler::update_member_role))
        .route("/im/groups/{group_id}/members/{member_id}/alias", axum::routing::put(im_group_handler::update_member_alias))
        .route("/im/groups/{group_id}/dissolve", axum::routing::delete(im_group_handler::dissolve_group))
        .route("/im/groups/{group_id}/delete", axum::routing::delete(im_group_handler::delete_group))
        .route("/im/groups/{group_id}", axum::routing::put(im_group_handler::update_group))
        // IM 发件箱相关路由
        .route("/im/outbox", axum::routing::post(im_outbox_handler::create_outbox))
        .route("/im/outbox/pending", axum::routing::get(im_outbox_handler::get_pending_messages))
        .route("/im/outbox/failed", axum::routing::get(im_outbox_handler::get_failed_messages))
        .route("/im/outbox/{id}", axum::routing::get(im_outbox_handler::get_outbox))
        .route("/im/outbox/{id}/status", axum::routing::put(im_outbox_handler::update_outbox_status))
        .route("/im/outbox/{id}/sent", axum::routing::post(im_outbox_handler::mark_sent))
        // 文件上传路由（需要认证）
        .route("/upload", axum::routing::post(upload_handler::upload_file))
        // 文件下载路由（需要认证，验证文件所有权）
        // 使用通配符路径匹配，支持包含 / 的文件路径（如 open_id/file_name）
        .route("/upload/{*path}", axum::routing::get(upload_handler::get_file))
        // WebRTC 路由（需要认证）
        .route("/webrtc/token", axum::routing::get(webrtc_handler::get_webrtc_token))
        .route("/webrtc/publish", axum::routing::get(webrtc_handler::get_webrtc_publish_token))
        .route("/webrtc/play", axum::routing::get(webrtc_handler::get_webrtc_play_token))
        // WebRTC SDP 交换代理（避免 CORS 问题）
        .route("/webrtc/rtc/v1/publish", axum::routing::post(webrtc_handler::proxy_rtc_publish))
        .route("/webrtc/rtc/v1/play", axum::routing::post(webrtc_handler::proxy_rtc_play))
        // WebRTC 查询流的播放者数量（用于判断对方是否接听）
        .route("/webrtc/stream/players", axum::routing::get(webrtc_handler::check_stream_players))
        .layer(middleware::from_fn(auth_middleware))
        .layer(Extension(pool))
        .layer(Extension(jwt_cfg))
        .layer(Extension(upload_settings))
        .layer(Extension(srs_settings))
        .layer(Extension(redis_client))
        .with_state((publisher, subscription_service))
}

