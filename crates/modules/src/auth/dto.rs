//! Auth DTOs — wire-level shapes that must stay byte-compatible with NestJS
//! so the Vue web frontend and Flutter app can switch backends transparently.

use serde::{Deserialize, Serialize};
use validator::Validate;

#[derive(Debug, Deserialize, Validate)]
pub struct LoginDto {
    #[validate(length(min = 1, max = 64))]
    pub username: String,
    #[validate(length(min = 1, max = 128))]
    pub password: String,
    #[serde(default)]
    pub captcha_id: Option<String>,
    #[serde(default)]
    pub captcha_code: Option<String>,
}

/// Matches NestJS `LoginTokenResponseDto` — **snake_case** on the wire
/// (unlike most DTOs which are camelCase), because the legacy backend uses
/// these exact field names.
#[derive(Debug, Serialize)]
pub struct LoginTokenResponseDto {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    pub expire_in: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_expire_in: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openid: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CaptchaCodeResponseDto {
    pub uuid: String,
    /// Base64-encoded SVG/PNG. Phase 0 stub returns an empty string.
    pub img: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUserInfoResponseDto {
    pub user_id: String,
    pub user_name: String,
    pub nick_name: String,
    pub avatar: String,
    pub email: String,
    pub phonenumber: String,
    pub user_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub platform_id: Option<String>,
    pub is_admin: bool,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,
}
