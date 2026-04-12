//! Domain-specific constants. Platform-level identity values
//! (`PLATFORM_ID_DEFAULT`, `USER_TYPE_CUSTOM`, `USER_TYPE_CLIENT`)
//! live in `framework::constants` because middleware needs them too.

/// Literal `user_name` of the system super admin row. The super admin
/// is the union of `user_name = SUPER_ADMIN_USERNAME` AND
/// `platform_id = framework::constants::PLATFORM_ID_DEFAULT` — both
/// must match. Used by `UserRepo::is_super_admin` guards across write
/// endpoints.
pub const SUPER_ADMIN_USERNAME: &str = "admin";

/// The fixed `tenant_id` of the built-in system (super) tenant. This
/// tenant is treated as the root of the tenant tree and must not be
/// modified or deleted by normal operations.
pub const SUPER_TENANT_ID: &str = "000000";
