//! Domain layer — entity structs (sqlx rows) and repositories.
//!
//! Audit + tenant-scope helpers live in `framework::context`
//! (pure `RequestContext` readers with no domain knowledge).

pub mod audit_log_repo;
pub mod config_repo;
pub mod constants;
pub mod dept_repo;
pub mod dict_data_repo;
pub mod dict_type_repo;
pub mod entities;
pub mod login_log_repo;
pub mod mail_account_repo;
pub mod mail_log_repo;
pub mod mail_template_repo;
pub mod menu_repo;
pub mod notice_repo;
pub mod notify_message_repo;
pub mod notify_template_repo;
pub mod oper_log_repo;
pub mod post_repo;
pub mod role_repo;
pub mod sms_channel_repo;
pub mod sms_log_repo;
pub mod sms_template_repo;
pub mod tenant_package_repo;
pub mod tenant_repo;
pub mod user_repo;
pub mod validators;

pub use audit_log_repo::{AuditLogListFilter, AuditLogRepo};
pub use config_repo::{ConfigInsertParams, ConfigListFilter, ConfigRepo, ConfigUpdateParams};
pub use dept_repo::{DeptInsertParams, DeptListFilter, DeptRepo, DeptUpdateParams};
pub use dict_data_repo::{
    DictDataInsertParams, DictDataListFilter, DictDataRepo, DictDataUpdateParams,
};
pub use dict_type_repo::{
    DictTypeInsertParams, DictTypeListFilter, DictTypeRepo, DictTypeUpdateParams,
};
pub use entities::{
    SysAuditLog, SysConfig, SysDept, SysDictData, SysDictType, SysLogininfor, SysMailAccount,
    SysMailLog, SysMailTemplate, SysMenu, SysNotice, SysNotifyMessage, SysNotifyTemplate,
    SysOperLog, SysPost, SysRole, SysSmsChannel, SysSmsLog, SysSmsTemplate, SysTenant,
    SysTenantPackage, SysUser, SysUserTenant,
};
pub use login_log_repo::{LoginLogListFilter, LoginLogRepo};
pub use mail_account_repo::{
    MailAccountInsertParams, MailAccountListFilter, MailAccountRepo, MailAccountUpdateParams,
};
pub use mail_log_repo::{MailLogListFilter, MailLogRepo};
pub use mail_template_repo::{
    MailTemplateInsertParams, MailTemplateListFilter, MailTemplateRepo, MailTemplateUpdateParams,
};
pub use menu_repo::{
    MenuInsertParams, MenuListFilter, MenuRepo, MenuTreeRow, MenuUpdateParams, RoleMenuTreeRow,
};
pub use notice_repo::{NoticeInsertParams, NoticeListFilter, NoticeRepo, NoticeUpdateParams};
pub use notify_message_repo::{
    NotifyMessageInsertParams, NotifyMessageListFilter, NotifyMessageRepo, NotifyMyMessageFilter,
};
pub use notify_template_repo::{
    NotifyTemplateInsertParams, NotifyTemplateListFilter, NotifyTemplateRepo,
    NotifyTemplateUpdateParams,
};
pub use oper_log_repo::{OperLogListFilter, OperLogRepo};
pub use post_repo::{PostInsertParams, PostListFilter, PostRepo, PostUpdateParams};
pub use role_repo::{
    AllocatedUserFilter, RoleInsertParams, RoleListFilter, RoleRepo, RoleUpdateParams,
};
pub use sms_channel_repo::{
    SmsChannelInsertParams, SmsChannelListFilter, SmsChannelRepo, SmsChannelUpdateParams,
};
pub use sms_log_repo::{SmsLogListFilter, SmsLogRepo};
pub use sms_template_repo::{
    SmsTemplateInsertParams, SmsTemplateListFilter, SmsTemplateRepo, SmsTemplateUpdateParams,
};
pub use tenant_package_repo::{
    PackageInsertParams, PackageListFilter, PackageUpdateParams, TenantPackageRepo,
};
pub use tenant_repo::{
    AdminUserInfo, TenantInsertParams, TenantListFilter, TenantRepo, TenantUpdateParams,
    TenantWithPackageName,
};
pub use user_repo::{UserInsertParams, UserListFilter, UserRepo, UserUpdateParams};
