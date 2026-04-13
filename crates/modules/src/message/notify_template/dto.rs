//! Notify template DTOs — wire shapes for `sys_notify_template` endpoints.

use crate::domain::validators::{default_status, validate_status_flag};
use crate::domain::SysNotifyTemplate;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyTemplateResponseDto {
    pub id: i32,
    pub name: String,
    pub code: String,
    pub nickname: String,
    pub content: String,
    pub params: Option<String>,
    pub r#type: i32,
    pub status: String,
    pub remark: Option<String>,
    pub create_by: String,
    pub create_at: String,
    pub update_by: String,
    pub update_at: String,
    pub i18n: Option<serde_json::Value>,
}

impl NotifyTemplateResponseDto {
    pub fn from_entity(t: SysNotifyTemplate) -> Self {
        Self {
            id: t.id,
            name: t.name,
            code: t.code,
            nickname: t.nickname,
            content: t.content,
            params: t.params,
            r#type: t.r#type,
            status: t.status,
            remark: t.remark,
            create_by: t.create_by,
            create_at: fmt_ts(&t.create_at),
            update_by: t.update_by,
            update_at: fmt_ts(&t.update_at),
            i18n: t.i18n,
        }
    }
}

/// Lightweight DTO for dropdown select.
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyTemplateOptionDto {
    pub id: i32,
    pub name: String,
    pub code: String,
    pub nickname: String,
}

impl NotifyTemplateOptionDto {
    pub fn from_entity(t: SysNotifyTemplate) -> Self {
        Self {
            id: t.id,
            name: t.name,
            code: t.code,
            nickname: t.nickname,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct CreateNotifyTemplateDto {
    #[validate(length(min = 1, max = 100))]
    pub name: String,
    #[validate(length(min = 1, max = 100))]
    pub code: String,
    #[validate(length(min = 1, max = 100))]
    pub nickname: String,
    pub content: String,
    pub params: Option<String>,
    #[serde(default = "default_type")]
    pub r#type: i32,
    #[serde(default = "default_status")]
    #[validate(custom(function = "validate_status_flag"))]
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNotifyTemplateDto {
    pub id: i32,
    #[validate(length(min = 1, max = 100))]
    pub name: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub code: Option<String>,
    #[validate(length(min = 1, max = 100))]
    pub nickname: Option<String>,
    pub content: Option<String>,
    pub params: Option<Option<String>>,
    pub r#type: Option<i32>,
    #[validate(custom(function = "validate_status_flag"))]
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListNotifyTemplateDto {
    pub name: Option<String>,
    pub r#type: Option<i32>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

fn default_type() -> i32 {
    1
}
