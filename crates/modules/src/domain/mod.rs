//! Domain layer — entity structs (sqlx rows) and repositories.
//!
//! Phase 0 models the tables needed for login → /info flow. Phase 1
//! adds `role_repo`. Audit + tenant-scope helpers that used to live
//! here under `common.rs` have been promoted to `framework::context`
//! (they're pure `RequestContext` readers with no domain knowledge).

pub mod config_repo;
pub mod constants;
pub mod dept_repo;
pub mod dict_data_repo;
pub mod dict_type_repo;
pub mod entities;
pub mod login_log_repo;
pub mod menu_repo;
pub mod notice_repo;
pub mod oper_log_repo;
pub mod post_repo;
pub mod role_repo;
pub mod tenant_package_repo;
pub mod tenant_repo;
pub mod user_repo;
pub mod validators;

pub use config_repo::{ConfigInsertParams, ConfigListFilter, ConfigRepo, ConfigUpdateParams};
pub use dept_repo::{DeptInsertParams, DeptListFilter, DeptRepo, DeptUpdateParams};
pub use dict_data_repo::{
    DictDataInsertParams, DictDataListFilter, DictDataRepo, DictDataUpdateParams,
};
pub use dict_type_repo::{
    DictTypeInsertParams, DictTypeListFilter, DictTypeRepo, DictTypeUpdateParams,
};
pub use entities::{
    SysConfig, SysDept, SysDictData, SysDictType, SysLogininfor, SysMenu, SysNotice, SysOperLog,
    SysPost, SysRole, SysTenant, SysTenantPackage, SysUser, SysUserTenant,
};
pub use login_log_repo::{LoginLogListFilter, LoginLogRepo};
pub use menu_repo::{
    MenuInsertParams, MenuListFilter, MenuRepo, MenuTreeRow, MenuUpdateParams, RoleMenuTreeRow,
};
pub use notice_repo::{NoticeInsertParams, NoticeListFilter, NoticeRepo, NoticeUpdateParams};
pub use oper_log_repo::{OperLogListFilter, OperLogRepo};
pub use post_repo::{PostInsertParams, PostListFilter, PostRepo, PostUpdateParams};
pub use role_repo::{
    AllocatedUserFilter, RoleInsertParams, RoleListFilter, RoleRepo, RoleUpdateParams,
};
pub use tenant_package_repo::{
    PackageInsertParams, PackageListFilter, PackageUpdateParams, TenantPackageRepo,
};
pub use tenant_repo::{
    AdminUserInfo, TenantInsertParams, TenantListFilter, TenantRepo, TenantUpdateParams,
    TenantWithPackageName,
};
pub use user_repo::{UserInsertParams, UserListFilter, UserRepo, UserUpdateParams};
