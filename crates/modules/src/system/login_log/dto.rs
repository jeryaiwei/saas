//! LoginLog DTOs — wire shapes for `sys_logininfor` endpoints.

use crate::domain::entities::SysLogininfor;
use framework::response::{fmt_ts, PageQuery};
use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Response DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LoginLogResponseDto {
    pub info_id: String,
    pub tenant_id: String,
    pub user_name: String,
    pub ipaddr: String,
    pub login_location: String,
    pub browser: String,
    pub os: String,
    pub device_type: String,
    pub status: String,
    pub msg: String,
    pub login_time: String,
}

impl LoginLogResponseDto {
    pub fn from_entity(e: SysLogininfor) -> Self {
        Self {
            info_id: e.info_id,
            tenant_id: e.tenant_id,
            user_name: e.user_name,
            ipaddr: e.ipaddr,
            login_location: e.login_location,
            browser: e.browser,
            os: e.os,
            device_type: e.device_type,
            status: e.status,
            msg: e.msg,
            login_time: fmt_ts(&e.login_time),
        }
    }
}

// ---------------------------------------------------------------------------
// Request DTOs
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct ListLoginLogDto {
    pub user_name: Option<String>,
    pub ipaddr: Option<String>,
    pub status: Option<String>,
    #[serde(flatten)]
    #[validate(nested)]
    pub page: PageQuery,
}
