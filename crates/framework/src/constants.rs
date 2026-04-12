//! Platform-level identity constants shared between the framework
//! layer (middleware, access checks) and the modules layer (repos,
//! services). These live in `framework/` rather than in any single
//! module because middleware needs them to enforce `Role::SuperAdmin`
//! / `Role::SuperTenant` / `Scope::Client` checks without reaching
//! into the `modules` crate.
//!
//! Phase 1 assumption: single-platform deployment. When multi-platform
//! support lands in Phase 2, `PLATFORM_ID_DEFAULT` becomes a runtime
//! lookup (probably from `RequestContext`) instead of a compile-time
//! constant.

/// Default platform id. All Phase 1 deployments run under a single
/// platform with this id — the value also doubles as the super-tenant
/// id (`SUPER_TENANT_ID` in NestJS parlance) for role checks like
/// `Role::SuperAdmin` (`is_admin && tenant_id == "000000"`).
pub const PLATFORM_ID_DEFAULT: &str = "000000";

/// `sys_user.user_type` for CUSTOM (backend admin) users.
pub const USER_TYPE_CUSTOM: &str = "10";

/// `sys_user.user_type` for CLIENT (C-end / mobile-app) users.
pub const USER_TYPE_CLIENT: &str = "20";
