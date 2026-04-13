//! SMS log DTOs — wire shapes for `sys_sms_log` endpoints.

use crate::domain::SysSmsLog;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SmsLogResponseDto {
    pub id: i64,
    pub channel_id: i32,
    pub channel_code: String,
    pub template_id: i32,
    pub template_code: String,
    pub mobile: String,
    pub content: String,
    pub params: Option<String>,
    pub send_status: i32,
    pub send_time: Option<String>,
    pub receive_status: Option<i32>,
    pub receive_time: Option<String>,
    pub api_send_code: Option<String>,
    pub api_receive_code: Option<String>,
    pub error_msg: Option<String>,
}

impl SmsLogResponseDto {
    pub fn from_entity(l: SysSmsLog) -> Self {
        Self {
            id: l.id,
            channel_id: l.channel_id,
            channel_code: l.channel_code,
            template_id: l.template_id,
            template_code: l.template_code,
            mobile: l.mobile,
            content: l.content,
            params: l.params,
            send_status: l.send_status,
            send_time: l.send_time.as_ref().map(fmt_ts),
            receive_status: l.receive_status,
            receive_time: l.receive_time.as_ref().map(fmt_ts),
            api_send_code: l.api_send_code,
            api_receive_code: l.api_receive_code,
            error_msg: l.error_msg,
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListSmsLogDto {
    pub mobile: Option<String>,
    pub template_code: Option<String>,
    pub send_status: Option<i32>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
