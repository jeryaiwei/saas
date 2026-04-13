//! Auth HTTP handlers + router.
//!
//! Routes:
//! - `POST /auth/login`          — public (whitelisted by `framework::middleware::auth`)
//! - `GET  /auth/code`           — public
//! - `GET  /auth/tenant/list`    — public
//! - `POST /auth/refresh-token`  — public
//! - `POST /auth/logout`         — authenticated
//! - `GET  /info`                — authenticated
//! - `GET  /routers`             — authenticated (menu tree for current user)

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Extension, State};
use framework::auth::{JwtClaims, UserSession};
use framework::error::AppError;
use framework::extractors::ValidatedJson;
use framework::response::ApiResponse;
use utoipa_axum::router::OpenApiRouter;
use utoipa_axum::routes;

#[utoipa::path(post, path = "/auth/login", tag = "认证",
    summary = "用户登录",
    request_body = dto::LoginDto,
    responses((status = 200, body = ApiResponse<dto::LoginTokenResponseDto>))
)]
pub(crate) async fn login(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::LoginDto>,
) -> Result<ApiResponse<dto::LoginTokenResponseDto>, AppError> {
    let resp = service::login(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/auth/code", tag = "认证",
    summary = "获取验证码",
    responses((status = 200, body = ApiResponse<dto::CaptchaCodeResponseDto>))
)]
pub(crate) async fn get_code(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::CaptchaCodeResponseDto>, AppError> {
    let resp = service::get_captcha(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/auth/logout", tag = "认证",
    summary = "退出登录",
    responses((status = 200, description = "success"))
)]
pub(crate) async fn logout(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<ApiResponse<()>, AppError> {
    service::logout(&state, &claims).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/info", tag = "认证",
    summary = "获取当前用户信息",
    responses((status = 200, body = ApiResponse<dto::CurrentUserInfoResponseDto>))
)]
pub(crate) async fn get_info(
    State(state): State<AppState>,
    Extension(session): Extension<UserSession>,
) -> Result<ApiResponse<dto::CurrentUserInfoResponseDto>, AppError> {
    let resp = service::get_info(&state, &session).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/auth/tenant/list", tag = "认证",
    summary = "获取租户列表",
    responses((status = 200, body = ApiResponse<dto::TenantListForLoginDto>))
)]
pub(crate) async fn tenant_list(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::TenantListForLoginDto>, AppError> {
    let resp = service::tenant_list(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/auth/refresh-token", tag = "认证",
    summary = "刷新令牌",
    request_body = dto::RefreshTokenDto,
    responses((status = 200, body = ApiResponse<dto::LoginTokenResponseDto>))
)]
pub(crate) async fn refresh_token(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::RefreshTokenDto>,
) -> Result<ApiResponse<dto::LoginTokenResponseDto>, AppError> {
    let resp = service::refresh_token(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/routers", tag = "认证",
    summary = "获取路由菜单树",
    responses((status = 200, description = "router tree JSON"))
)]
pub(crate) async fn get_routers(
    State(state): State<AppState>,
    Extension(session): Extension<UserSession>,
) -> Result<ApiResponse<serde_json::Value>, AppError> {
    let routers = service::get_routers(&state, &session).await?;
    let value = serde_json::to_value(&routers)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("serialize routers: {e}")))?;
    Ok(ApiResponse::ok(value))
}

pub fn router() -> OpenApiRouter<AppState> {
    // Split into two routers to reduce type nesting depth
    // (avoids stack overflow in debug mode from deep Future state machines)
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(get_code))
        .routes(routes!(tenant_list))
        .routes(routes!(refresh_token))
        .routes(routes!(logout))
        .routes(routes!(get_info))
        .routes(routes!(get_routers))
}
