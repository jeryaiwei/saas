//! Domain-specific constants. Platform-level identity values
//! (`PLATFORM_ID_DEFAULT`, `USER_TYPE_CUSTOM`, `USER_TYPE_CLIENT`)
//! live in `framework::constants` because middleware needs them too.

/// Literal `user_name` of the system super admin row. The super admin
/// is the union of `user_name = SUPER_ADMIN_USERNAME` AND
/// `platform_id = framework::constants::PLATFORM_ID_DEFAULT` — both
/// must match. Used by `UserRepo::is_super_admin` guards across write
/// endpoints.
pub const SUPER_ADMIN_USERNAME: &str = "admin";
