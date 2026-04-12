//! Tenant Package DTOs — wire shapes for `sys_tenant_package` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysTenantPackage;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Full package detail. Returned by `GET /system/tenant-package/{id}` and
/// `POST /system/tenant-package/`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageDetailResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
    pub menu_ids: Vec<String>,
    pub menu_check_strictly: bool,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl PackageDetailResponseDto {
    pub fn from_entity(p: SysTenantPackage) -> Self {
        Self {
            package_id: p.package_id,
            code: p.code,
            package_name: p.package_name,
            menu_ids: p.menu_ids,
            menu_check_strictly: p.menu_check_strictly,
            status: p.status,
            create_by: p.create_by,
            create_at: fmt_ts(&p.create_at),
            update_by: p.update_by,
            update_at: fmt_ts(&p.update_at),
            remark: p.remark,
        }
    }
}

/// Lightweight row shape for the package list page.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageListItemResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
    pub status: String,
    pub create_at: String,
}

impl PackageListItemResponseDto {
    pub fn from_entity(p: SysTenantPackage) -> Self {
        Self {
            package_id: p.package_id,
            code: p.code,
            package_name: p.package_name,
            status: p.status,
            create_at: fmt_ts(&p.create_at),
        }
    }
}

/// Dropdown-optimized flat projection. Returned by
/// `GET /system/tenant-package/option-select`.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PackageOptionResponseDto {
    pub package_id: String,
    pub code: String,
    pub package_name: String,
}

impl PackageOptionResponseDto {
    pub fn from_entity(p: SysTenantPackage) -> Self {
        Self {
            package_id: p.package_id,
            code: p.code,
            package_name: p.package_name,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Request body for `POST /system/tenant-package/`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreatePackageDto {
    #[validate(length(min = 1, max = 20))]
    pub code: String,
    #[validate(length(min = 1, max = 50))]
    pub package_name: String,
    #[serde(default)]
    pub menu_ids: Vec<String>,
    #[serde(default)]
    pub menu_check_strictly: bool,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

/// Request body for `PUT /system/tenant-package/`.
/// All non-id fields are `Option` — only the provided fields are updated.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePackageDto {
    #[validate(length(min = 1, max = 36))]
    pub package_id: String,
    #[validate(length(min = 1, max = 20))]
    pub code: Option<String>,
    #[validate(length(min = 1, max = 50))]
    pub package_name: Option<String>,
    pub menu_ids: Option<Vec<String>>,
    pub menu_check_strictly: Option<bool>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    #[validate(length(max = 500))]
    pub remark: Option<String>,
}

/// Query string for `GET /system/tenant-package/list`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListPackageDto {
    pub package_name: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
