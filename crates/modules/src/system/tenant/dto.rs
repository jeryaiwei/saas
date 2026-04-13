//! Tenant DTOs — request/response wire shapes for tenant management.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::{AdminUserInfo, TenantWithPackageName};
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Full tenant detail returned by `GET /system/tenant/{id}`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantDetailResponseDto {
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
    pub package_name: Option<String>,
    pub expire_time: Option<String>,
    pub account_count: i32,
    pub language: String,
    pub admin_user_name: Option<String>,
    pub nick_name: Option<String>,
    pub phonenumber: Option<String>,
    pub whatsapp: Option<String>,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl TenantDetailResponseDto {
    /// For detail endpoint — includes admin contact fields from `AdminUserInfo`.
    pub fn from_entity_with_admin(
        tenant: TenantWithPackageName,
        admin: Option<AdminUserInfo>,
        admin_user_name: Option<String>,
    ) -> Self {
        let (nick_name, phonenumber, whatsapp) = match admin {
            Some(a) => (Some(a.nick_name), Some(a.phonenumber), Some(a.whatsapp)),
            None => (None, None, None),
        };
        Self {
            id: tenant.id,
            tenant_id: tenant.tenant_id,
            parent_id: tenant.parent_id,
            contact_user_name: tenant.contact_user_name,
            contact_phone: tenant.contact_phone,
            company_name: tenant.company_name,
            license_number: tenant.license_number,
            address: tenant.address,
            intro: tenant.intro,
            domain: tenant.domain,
            package_id: tenant.package_id,
            package_name: tenant.package_name,
            expire_time: tenant.expire_time.as_ref().map(fmt_ts),
            account_count: tenant.account_count,
            language: tenant.language,
            admin_user_name,
            nick_name,
            phonenumber,
            whatsapp,
            status: tenant.status,
            create_by: tenant.create_by,
            create_at: fmt_ts(&tenant.create_at),
            update_by: tenant.update_by,
            update_at: fmt_ts(&tenant.update_at),
            remark: tenant.remark,
        }
    }

    /// For list endpoint — no admin contact detail, only the username.
    pub fn from_entity_with_admin_name(
        tenant: TenantWithPackageName,
        admin_user_name: Option<String>,
    ) -> Self {
        Self {
            id: tenant.id,
            tenant_id: tenant.tenant_id,
            parent_id: tenant.parent_id,
            contact_user_name: tenant.contact_user_name,
            contact_phone: tenant.contact_phone,
            company_name: tenant.company_name,
            license_number: tenant.license_number,
            address: tenant.address,
            intro: tenant.intro,
            domain: tenant.domain,
            package_id: tenant.package_id,
            package_name: tenant.package_name,
            expire_time: tenant.expire_time.as_ref().map(fmt_ts),
            account_count: tenant.account_count,
            language: tenant.language,
            admin_user_name,
            nick_name: None,
            phonenumber: None,
            whatsapp: None,
            status: tenant.status,
            create_by: tenant.create_by,
            create_at: fmt_ts(&tenant.create_at),
            update_by: tenant.update_by,
            update_at: fmt_ts(&tenant.update_at),
            remark: tenant.remark,
        }
    }
}

/// Lightweight row for `GET /system/tenant/list`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantListItemResponseDto {
    pub id: String,
    pub tenant_id: String,
    pub company_name: String,
    pub contact_user_name: Option<String>,
    pub contact_phone: Option<String>,
    pub package_name: Option<String>,
    pub admin_user_name: Option<String>,
    pub expire_time: Option<String>,
    pub account_count: i32,
    pub status: String,
    pub create_at: String,
}

impl TenantListItemResponseDto {
    pub fn from_entity(tenant: TenantWithPackageName, admin_user_name: Option<String>) -> Self {
        Self {
            id: tenant.id,
            tenant_id: tenant.tenant_id,
            company_name: tenant.company_name,
            contact_user_name: tenant.contact_user_name,
            contact_phone: tenant.contact_phone,
            package_name: tenant.package_name,
            admin_user_name,
            expire_time: tenant.expire_time.as_ref().map(fmt_ts),
            account_count: tenant.account_count,
            status: tenant.status,
            create_at: fmt_ts(&tenant.create_at),
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

fn default_account_count() -> i32 {
    -1
}

fn default_language() -> String {
    "zh-CN".into()
}

/// Request body for `POST /system/tenant/`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateTenantDto {
    #[validate(length(min = 1, max = 100))]
    pub company_name: String,

    #[validate(length(min = 1, max = 30))]
    pub username: String,

    #[validate(length(min = 6, max = 128))]
    pub password: String,

    #[validate(length(min = 1))]
    pub package_ids: Vec<String>,

    #[validate(length(min = 1, max = 20))]
    pub parent_id: Option<String>,

    #[validate(length(max = 50))]
    pub contact_user_name: Option<String>,

    #[validate(length(max = 20))]
    pub contact_phone: Option<String>,

    #[validate(length(max = 50))]
    pub license_number: Option<String>,

    #[validate(length(max = 200))]
    pub address: Option<String>,

    pub intro: Option<String>,

    #[validate(length(max = 100))]
    pub domain: Option<String>,

    pub expire_time: Option<String>,

    #[serde(default = "default_account_count")]
    pub account_count: i32,

    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,

    #[serde(default = "default_language")]
    #[validate(length(min = 2, max = 10))]
    pub language: String,

    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

/// Request body for `PUT /system/tenant/`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTenantDto {
    /// Surrogate PK (UUID). Used to find and update the record.
    #[validate(length(min = 1, max = 36))]
    pub id: String,

    /// Business tenant_id — for protected-tenant check. NOT updated.
    #[validate(length(min = 1, max = 20))]
    pub tenant_id: String,

    #[validate(length(max = 50))]
    pub contact_user_name: Option<String>,

    #[validate(length(max = 20))]
    pub contact_phone: Option<String>,

    #[validate(length(min = 1, max = 100))]
    pub company_name: Option<String>,

    #[validate(length(max = 50))]
    pub license_number: Option<String>,

    #[validate(length(max = 200))]
    pub address: Option<String>,

    pub intro: Option<String>,

    #[validate(length(max = 100))]
    pub domain: Option<String>,

    pub package_id: Option<String>,

    pub expire_time: Option<String>,

    pub account_count: Option<i32>,

    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,

    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

/// Query string for `GET /system/tenant/list`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListTenantDto {
    pub tenant_id: Option<String>,
    #[validate(length(max = 50))]
    pub contact_user_name: Option<String>,
    #[validate(length(max = 20))]
    pub contact_phone: Option<String>,
    #[validate(length(max = 100))]
    pub company_name: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

// ---------------------------------------------------------------------------
// Tenant select-list / switch DTOs
// ---------------------------------------------------------------------------

/// Lightweight tenant option returned by `GET /system/tenant/select-list`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantSelectOptionDto {
    pub tenant_id: String,
    pub company_name: String,
}

/// Response for `GET /system/tenant/switch-status`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantSwitchStatusDto {
    pub current_tenant_id: Option<String>,
    pub default_tenant_id: Option<String>,
    pub is_switched: bool,
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::{CreateTenantDto, ListTenantDto, UpdateTenantDto};
    use framework::response::PageQuery;
    use validator::Validate;

    fn valid_create() -> CreateTenantDto {
        CreateTenantDto {
            company_name: "Acme".into(),
            username: "admin".into(),
            password: "password123".into(),
            package_ids: vec!["pkg1".into()],
            parent_id: None,
            contact_user_name: None,
            contact_phone: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            expire_time: None,
            account_count: -1,
            status: "0".into(),
            language: "zh-CN".into(),
            remark: None,
        }
    }

    #[test]
    fn create_tenant_dto_accepts_valid_minimum() {
        assert!(valid_create().validate().is_ok());
    }

    #[test]
    fn create_tenant_dto_rejects_empty_company_name() {
        let mut dto = valid_create();
        dto.company_name = "".into();
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_tenant_dto_rejects_short_password() {
        let mut dto = valid_create();
        dto.password = "abc".into();
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_tenant_dto_rejects_empty_package_ids() {
        let mut dto = valid_create();
        dto.package_ids = vec![];
        assert!(dto.validate().is_err());
    }

    #[test]
    fn create_tenant_dto_rejects_invalid_status() {
        let mut dto = valid_create();
        dto.status = "x".into();
        assert!(dto.validate().is_err());
    }

    #[test]
    fn update_tenant_dto_rejects_empty_id() {
        let dto = UpdateTenantDto {
            id: "".into(),
            tenant_id: "000001".into(),
            contact_user_name: None,
            contact_phone: None,
            company_name: None,
            license_number: None,
            address: None,
            intro: None,
            domain: None,
            package_id: None,
            expire_time: None,
            account_count: None,
            status: None,
            remark: None,
        };
        assert!(dto.validate().is_err());
    }

    #[test]
    fn list_tenant_dto_accepts_defaults() {
        let dto = ListTenantDto {
            tenant_id: None,
            contact_user_name: None,
            contact_phone: None,
            company_name: None,
            status: None,
            page: PageQuery::default(),
        };
        assert!(dto.validate().is_ok());
    }

    #[test]
    fn list_tenant_dto_rejects_page_num_zero() {
        let dto = ListTenantDto {
            tenant_id: None,
            contact_user_name: None,
            contact_phone: None,
            company_name: None,
            status: None,
            page: PageQuery {
                page_num: 0,
                page_size: 10,
            },
        };
        assert!(dto.validate().is_err());
    }
}
