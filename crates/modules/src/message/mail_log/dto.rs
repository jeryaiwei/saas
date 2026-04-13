//! Mail log DTOs — wire shapes for `sys_mail_log` endpoints.

use crate::domain::SysMailLog;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct MailLogResponseDto {
    pub id: i64,
    pub user_id: Option<String>,
    pub user_type: Option<i32>,
    pub to_mail: String,
    pub account_id: i32,
    pub from_mail: String,
    pub template_id: i32,
    pub template_code: String,
    pub template_nickname: String,
    pub template_title: String,
    pub template_content: String,
    pub template_params: Option<String>,
    pub send_status: i32,
    pub send_time: Option<String>,
    pub error_msg: Option<String>,
}

impl MailLogResponseDto {
    pub fn from_entity(l: SysMailLog) -> Self {
        Self {
            id: l.id,
            user_id: l.user_id,
            user_type: l.user_type,
            to_mail: l.to_mail,
            account_id: l.account_id,
            from_mail: l.from_mail,
            template_id: l.template_id,
            template_code: l.template_code,
            template_nickname: l.template_nickname,
            template_title: l.template_title,
            template_content: l.template_content,
            template_params: l.template_params,
            send_status: l.send_status,
            send_time: l.send_time.as_ref().map(fmt_ts),
            error_msg: l.error_msg,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListMailLogDto {
    pub to_mail: Option<String>,
    pub template_code: Option<String>,
    pub send_status: Option<i32>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
