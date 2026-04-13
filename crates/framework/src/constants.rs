//! Platform-level identity constants shared between the framework
//! layer (middleware, access checks) and the modules layer (repos,
//! services).

/// 超级租户 ID。种子数据固定为 `"000000"`，系统中唯一。
/// 用于判断超级管理员、超级租户等角色。
pub const SUPER_TENANT_ID: &str = "000000";

/// 默认平台 ID — 与超级租户 ID 相同。
/// 语义上指"新用户的默认 platform_id"时使用。
pub const PLATFORM_ID_DEFAULT: &str = SUPER_TENANT_ID;

/// `sys_user.user_type` for CUSTOM (backend admin) users.
pub const USER_TYPE_CUSTOM: &str = "10";

/// `sys_user.user_type` for CLIENT (C-end / mobile-app) users.
pub const USER_TYPE_CLIENT: &str = "20";
