//! Auth HTTP handlers + router.
//!
//! Routes:
//! - `POST /auth/login`   — public (whitelisted by `framework::middleware::auth`)
//! - `GET  /auth/code`    — public
//! - `POST /auth/logout`  — authenticated
//! - `GET  /info`         — authenticated

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

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(login))
        .routes(routes!(get_code))
        .routes(routes!(logout))
        .routes(routes!(get_info))
}
