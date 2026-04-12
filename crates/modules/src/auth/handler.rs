//! Auth HTTP handlers + router.
//!
//! Routes:
//! - `POST /auth/login`   — public (whitelisted by `framework::middleware::auth`)
//! - `GET  /auth/code`    — public
//! - `POST /auth/logout`  — authenticated
//! - `GET  /info`         — authenticated

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Extension, State},
    routing::{get, post},
    Router,
};
use framework::auth::{JwtClaims, UserSession};
use framework::error::AppError;
use framework::extractors::ValidatedJson;
use framework::response::ApiResponse;

async fn login(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::LoginDto>,
) -> Result<ApiResponse<dto::LoginTokenResponseDto>, AppError> {
    let resp = service::login(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

async fn get_code(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::CaptchaCodeResponseDto>, AppError> {
    let resp = service::get_captcha(&state).await?;
    Ok(ApiResponse::ok(resp))
}

async fn logout(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<ApiResponse<()>, AppError> {
    service::logout(&state, &claims).await?;
    Ok(ApiResponse::success())
}

async fn get_info(
    State(state): State<AppState>,
    Extension(session): Extension<UserSession>,
) -> Result<ApiResponse<dto::CurrentUserInfoResponseDto>, AppError> {
    let resp = service::get_info(&state, &session).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/auth/login", post(login))
        .route("/auth/code", get(get_code))
        .route("/auth/logout", post(logout))
        .route("/info", get(get_info))
}
