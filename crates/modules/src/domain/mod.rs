//! Domain layer — entity structs (sqlx rows) and repositories.
//!
//! Phase 0 models the tables needed for login → /info flow. Phase 1
//! adds `role_repo`. Audit + tenant-scope helpers that used to live
//! here under `common.rs` have been promoted to `framework::context`
//! (they're pure `RequestContext` readers with no domain knowledge).

pub mod constants;
pub mod entities;
pub mod role_repo;
pub mod user_repo;
pub mod validators;

pub use entities::{SysRole, SysUser, SysUserTenant};
pub use role_repo::{
    AllocatedUserFilter, RoleInsertParams, RoleListFilter, RoleRepo, RoleUpdateParams,
};
pub use user_repo::{UserInsertParams, UserListFilter, UserRepo, UserUpdateParams};
