//! Notice DTOs — wire shapes for `sys_notice` endpoints.

use crate::domain::validators::{default_status, validate_notice_type, validate_status_flag};
use crate::domain::SysNotice;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NoticeResponseDto {
    pub notice_id: String,
    pub notice_title: String,
    pub notice_type: String,
    pub notice_content: Option<String>,
    pub status: String,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub remark: Option<String>,
}

impl NoticeResponseDto {
    pub fn from_entity(n: SysNotice) -> Self {
        Self {
            notice_id: n.notice_id,
            notice_title: n.notice_title,
            notice_type: n.notice_type,
            notice_content: n.notice_content,
            status: n.status,
            create_by: n.create_by,
            create_at: fmt_ts(&n.create_at),
            update_by: n.update_by,
            update_at: fmt_ts(&n.update_at),
            remark: n.remark,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoticeDto {
    #[validate(length(min = 1, max = 50))]
    pub notice_title: String,
    #[validate(custom(function = "validate_notice_type"))]
    pub notice_type: String,
    pub notice_content: Option<String>,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoticeDto {
    pub notice_id: String,
    #[validate(length(min = 1, max = 50))]
    pub notice_title: Option<String>,
    #[validate(custom(function = "validate_notice_type"))]
    pub notice_type: Option<String>,
    pub notice_content: Option<Option<String>>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListNoticeDto {
    pub notice_title: Option<String>,
    pub notice_type: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
