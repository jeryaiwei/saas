//! Dept DTOs — wire shapes for `sys_dept` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysDept;
use framework::response::fmt_ts;
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

/// Full dept detail. Returned by most dept endpoints.
/// Excludes `tenant_id`, `del_flag`, and `i18n` (internal fields).
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeptResponseDto {
    pub dept_id: String,
    pub parent_id: Option<String>,
    pub ancestors: Vec<String>,
    pub dept_name: String,
    pub order_num: i32,
    pub leader: String,
    pub phone: String,
    pub email: String,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl DeptResponseDto {
    pub fn from_entity(d: SysDept) -> Self {
        Self {
            dept_id: d.dept_id,
            parent_id: d.parent_id,
            ancestors: d.ancestors,
            dept_name: d.dept_name,
            order_num: d.order_num,
            leader: d.leader,
            phone: d.phone,
            email: d.email,
            status: d.status,
            create_by: d.create_by,
            create_at: fmt_ts(&d.create_at),
            update_by: d.update_by,
            update_at: fmt_ts(&d.update_at),
            remark: d.remark,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

/// Request body for `POST /system/dept/`.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateDeptDto {
    /// Use `"0"` for root department.
    pub parent_id: String,
    #[validate(length(min = 1, max = 30))]
    pub dept_name: String,
    #[validate(range(min = 0))]
    #[serde(default)]
    pub order_num: i32,
    #[validate(length(max = 20))]
    pub leader: Option<String>,
    #[validate(length(max = 11))]
    pub phone: Option<String>,
    #[validate(email)]
    pub email: Option<String>,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

/// Request body for `PUT /system/dept/`.
/// `dept_id` and `parent_id` are required; all other fields are optional.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateDeptDto {
    pub dept_id: String,
    /// Still required per API contract (use `"0"` for root).
    pub parent_id: String,
    #[validate(length(min = 1, max = 30))]
    pub dept_name: Option<String>,
    #[validate(range(min = 0))]
    pub order_num: Option<i32>,
    #[validate(length(max = 20))]
    pub leader: Option<String>,
    #[validate(length(max = 11))]
    pub phone: Option<String>,
    #[validate(email)]
    pub email: Option<String>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<String>,
}

/// Query string for `GET /system/dept/list`. Non-paginated.
#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListDeptDto {
    pub dept_name: Option<String>,
    pub status: Option<String>,
}
