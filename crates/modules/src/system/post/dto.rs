//! Post DTOs — wire shapes for `sys_post` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysPost;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PostResponseDto {
    pub post_id: String,
    pub dept_id: Option<String>,
    pub post_code: String,
    pub post_category: Option<String>,
    pub post_name: String,
    pub post_sort: i32,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl PostResponseDto {
    pub fn from_entity(p: SysPost) -> Self {
        Self {
            post_id: p.post_id,
            dept_id: p.dept_id,
            post_code: p.post_code,
            post_category: p.post_category,
            post_name: p.post_name,
            post_sort: p.post_sort,
            status: p.status,
            create_by: p.create_by,
            create_at: fmt_ts(&p.create_at),
            update_by: p.update_by,
            update_at: fmt_ts(&p.update_at),
            remark: p.remark,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreatePostDto {
    #[validate(length(min = 1, max = 64))]
    pub post_code: String,
    #[validate(length(min = 1, max = 50))]
    pub post_name: String,
    #[validate(length(max = 100))]
    pub post_category: Option<String>,
    pub dept_id: Option<String>,
    #[serde(default)]
    pub post_sort: i32,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdatePostDto {
    pub post_id: String,
    #[validate(length(min = 1, max = 64))]
    pub post_code: Option<String>,
    #[validate(length(min = 1, max = 50))]
    pub post_name: Option<String>,
    #[validate(length(max = 100))]
    pub post_category: Option<Option<String>>,
    pub dept_id: Option<Option<String>>,
    pub post_sort: Option<i32>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListPostDto {
    pub post_name: Option<String>,
    pub post_code: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
