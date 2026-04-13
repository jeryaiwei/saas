//! Auth DTOs — wire-level shapes that must stay byte-compatible with NestJS
//! so the Vue web frontend and Flutter app can switch backends transparently.

use serde::{Deserialize, Serialize};
use validator::Validate;

// ---------------------------------------------------------------------------
// Tenant list for login page
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantListForLoginDto {
    pub tenant_enabled: bool,
    pub vo_list: Vec<TenantVo>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TenantVo {
    pub tenant_id: String,
    pub company_name: String,
    pub domain: Option<String>,
}

// ---------------------------------------------------------------------------
// Refresh token
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct RefreshTokenDto {
    #[validate(length(min = 1))]
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, Validate, utoipa::ToSchema, utoipa::IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct LoginDto {
    #[validate(length(min = 1, max = 64))]
    pub username: String,
    #[validate(length(min = 1, max = 128))]
    pub password: String,
    #[serde(default)]
    pub captcha_id: Option<String>,
    #[serde(default)]
    pub captcha_code: Option<String>,
    /// Optional tenant selection for login. If omitted, uses the user's
    /// default tenant binding or falls back to platformId.
    #[serde(default)]
    pub tenant_id: Option<String>,
}

/// Matches NestJS `LoginTokenResponseDto` — **snake_case** on the wire
/// (unlike most DTOs which are camelCase), because the legacy backend uses
/// these exact field names.
#[derive(Debug, Serialize, utoipa::ToSchema)]
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

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct CaptchaCodeResponseDto {
    pub uuid: String,
    /// Base64-encoded SVG/PNG. Phase 0 stub returns an empty string.
    pub img: String,
}

// ---------------------------------------------------------------------------
// Router menu tree response (GET /routers)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouterMeta {
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    pub no_cache: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub link: Option<String>,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RouterConfig {
    pub hidden: bool,
    pub name: String,
    pub path: String,
    pub component: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub meta: Option<RouterMeta>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub always_show: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub redirect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub children: Option<Vec<RouterConfig>>,
}

// ---------------------------------------------------------------------------
// Current user info
// ---------------------------------------------------------------------------

/// Nested user profile — matches NestJS frontend contract
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserProfileDto {
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
}

/// Response for GET /info — frontend expects { user, permissions, roles }
#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CurrentUserInfoResponseDto {
    pub user: UserProfileDto,
    pub roles: Vec<String>,
    pub permissions: Vec<String>,   
}
