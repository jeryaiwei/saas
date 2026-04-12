//! Phase 0 entity structs — only the fields we actually read during the
//! login and /info flows. Extend as new endpoints land.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

/// Full `sys_user` row. Mirrors the DB schema including the bcrypt
/// password hash. `Serialize` is derived but `password` is skipped to
/// prevent accidental leakage; wire responses should use explicit DTOs.
///
/// `Debug` is NOT derived — a manual impl redacts the `password` field
/// so `tracing::debug!("{user:?}")` can never leak the hash to logs.
#[derive(Clone, FromRow, Serialize)]
pub struct SysUser {
    pub user_id: String,
    pub platform_id: String,
    pub dept_id: Option<String>,
    pub user_name: String,
    pub nick_name: String,
    pub user_type: String,
    pub client_type: Option<String>,
    pub lang: Option<String>,
    pub email: String,
    pub phonenumber: String,
    pub whatsapp: String,
    pub sex: String,
    pub avatar: String,
    #[serde(skip_serializing)]
    pub password: String,
    pub status: String,
    pub del_flag: String,
    pub login_ip: String,
    pub login_date: Option<DateTime<Utc>>,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysUserTenant {
    pub id: String,
    pub user_id: String,
    pub tenant_id: String,
    pub is_default: String,
    pub is_admin: String,
    pub status: String,
}

impl SysUser {
    pub fn is_active(&self) -> bool {
        self.del_flag == "0" && self.status == "0"
    }
}

impl std::fmt::Debug for SysUser {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SysUser")
            .field("user_id", &self.user_id)
            .field("platform_id", &self.platform_id)
            .field("dept_id", &self.dept_id)
            .field("user_name", &self.user_name)
            .field("nick_name", &self.nick_name)
            .field("user_type", &self.user_type)
            .field("client_type", &self.client_type)
            .field("lang", &self.lang)
            .field("email", &self.email)
            .field("phonenumber", &self.phonenumber)
            .field("whatsapp", &self.whatsapp)
            .field("sex", &self.sex)
            .field("avatar", &self.avatar)
            .field("password", &"[REDACTED]")
            .field("status", &self.status)
            .field("del_flag", &self.del_flag)
            .field("login_ip", &self.login_ip)
            .field("login_date", &self.login_date)
            .field("create_by", &self.create_by)
            .field("create_at", &self.create_at)
            .field("update_by", &self.update_by)
            .field("update_at", &self.update_at)
            .field("remark", &self.remark)
            .finish()
    }
}

impl SysUserTenant {
    pub fn is_admin_flag(&self) -> bool {
        self.is_admin == "1"
    }
    pub fn is_default_flag(&self) -> bool {
        self.is_default == "1"
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysRole {
    pub role_id: String,
    pub tenant_id: String,
    pub role_name: String,
    pub role_key: String,
    pub role_sort: i32,
    pub data_scope: String,
    pub menu_check_strictly: bool,
    pub dept_check_strictly: bool,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: chrono::DateTime<chrono::Utc>,
    pub update_by: String,
    pub update_at: chrono::DateTime<chrono::Utc>,
    pub remark: Option<String>,
}

impl SysRole {
    pub fn is_active(&self) -> bool {
        self.del_flag == "0" && self.status == "0"
    }
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysTenant {
    pub id: String,
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub company_name: String,
    pub license_number: Option<String>,
    pub address: Option<String>,
    pub intro: Option<String>,
    pub domain: Option<String>,
    pub package_id: Option<String>,
    pub expire_time: Option<DateTime<Utc>>,
    pub account_count: i32,
    pub storage_quota: i32,
    pub storage_used: i32,
    pub api_quota: i32,
    pub language: String,
    pub verify_status: Option<String>,
    pub license_image_url: Option<String>,
    pub reject_reason: Option<String>,
    pub verified_at: Option<DateTime<Utc>>,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysTenantPackage {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
    pub menu_ids: Vec<String>,
    pub menu_check_strictly: bool,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysDept {
    pub dept_id: String,
    pub tenant_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Vec<String>,
    pub dept_name: String,
    pub order_num: i32,
    pub leader: String,
    pub phone: String,
    pub email: String,
    pub status: String,
    pub del_flag: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
    pub i18n: Option<serde_json::Value>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct SysMenu {
    pub menu_id: String,
    pub menu_name: String,
    pub parent_id: Option<String>,
    pub order_num: i32,
    pub path: String,
    pub component: Option<String>,
    pub query: String,
    pub is_frame: String,
    pub is_cache: String,
    pub menu_type: String,
    pub visible: String,
    pub status: String,
    pub perms: String,
    pub icon: String,
    pub create_by: String,
    pub create_at: DateTime<Utc>,
    pub update_by: String,
    pub update_at: DateTime<Utc>,
    pub remark: Option<String>,
    pub del_flag: String,
    pub i18n: Option<serde_json::Value>,
}
