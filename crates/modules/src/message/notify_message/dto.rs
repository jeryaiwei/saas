//! Notify message DTOs — wire shapes for `sys_notify_message` endpoints.

use crate::domain::SysNotifyMessage;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NotifyMessageResponseDto {
    pub id: i64,
    pub tenant_id: String,
    pub user_id: String,
    pub user_type: i32,
    pub template_id: i32,
    pub template_code: String,
    pub template_nickname: String,
    pub template_content: String,
    pub template_params: Option<String>,
    pub read_status: bool,
    pub read_time: Option<String>,
    pub create_at: String,
    pub update_at: String,
}

impl NotifyMessageResponseDto {
    pub fn from_entity(m: SysNotifyMessage) -> Self {
        Self {
            id: m.id,
            tenant_id: m.tenant_id,
            user_id: m.user_id,
            user_type: m.user_type,
            template_id: m.template_id,
            template_code: m.template_code,
            template_nickname: m.template_nickname,
            template_content: m.template_content,
            template_params: m.template_params,
            read_status: m.read_status,
            read_time: m.read_time.as_ref().map(fmt_ts),
            create_at: fmt_ts(&m.create_at),
            update_at: fmt_ts(&m.update_at),
        }
    }
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnreadCountDto {
    pub count: i64,
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SendNotifyMessageDto {
    #[validate(length(min = 1))]
    pub user_id: String,
    #[serde(default = "default_user_type")]
    pub user_type: i32,
    pub template_id: i32,
    #[validate(length(min = 1, max = 100))]
    pub template_code: String,
    #[validate(length(min = 1, max = 100))]
    pub template_nickname: String,
    pub template_content: String,
    pub template_params: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct SendAllNotifyMessageDto {
    pub user_ids: Vec<String>,
    #[serde(default = "default_user_type")]
    pub user_type: i32,
    pub template_id: i32,
    #[validate(length(min = 1, max = 100))]
    pub template_code: String,
    #[validate(length(min = 1, max = 100))]
    pub template_nickname: String,
    pub template_content: String,
    pub template_params: Option<String>,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListNotifyMessageDto {
    pub template_code: Option<String>,
    pub user_id: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct MyNotifyMessageDto {
    pub read_status: Option<bool>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}

fn default_user_type() -> i32 {
    1
}
