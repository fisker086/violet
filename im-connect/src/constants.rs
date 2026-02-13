//! IM 业务常量，与 Lucky-cloud im-starter-core 对齐

#[allow(dead_code)]
pub const USER_CACHE_PREFIX: &str = "IM-USER-";
#[allow(dead_code)]
pub const IM_USER: &str = "userId";
#[allow(dead_code)]
pub const IM_DEVICE_TYPE: &str = "deviceType";
#[allow(dead_code)]
pub const BEARER_PREFIX: &str = "Bearer ";
#[allow(dead_code)]
pub const MQ_EXCHANGE_NAME: &str = "IM-SERVER";
#[allow(dead_code)]
pub const MQ_ROUTERKEY_PREFIX: &str = "IM-ROUTER-";

/// 消息类型 code，与 IMessageType 枚举一致
pub mod code {
    pub const ERROR: i32 = -1;
    #[allow(dead_code)]
    pub const SUCCESS: i32 = 0;
    pub const REGISTER: i32 = 200;
    pub const HEART_BEAT: i32 = 206;
    pub const HEART_BEAT_SUCCESS: i32 = 207;
    pub const REGISTER_SUCCESS: i32 = 209;
    pub const FORCE_LOGOUT: i32 = 104;
    pub const SINGLE_MESSAGE: i32 = 1000;
    pub const GROUP_MESSAGE: i32 = 1001;
    pub const VIDEO_MESSAGE: i32 = 1002;
    pub const GROUP_OPERATION: i32 = 1005;
    pub const MESSAGE_OPERATION: i32 = 1006;
}
