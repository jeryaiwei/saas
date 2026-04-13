//! Online user HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::operlog;
use framework::require_permission;
use framework::response::ApiResponse;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/monitor/online/list", tag = "在线用户",
    summary = "在线用户列表",
    responses((status = 200, body = ApiResponse<Vec<dto::OnlineUserResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::OnlineUserResponseDto>>, AppError> {
    let resp = service::list(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/monitor/online/{token}", tag = "在线用户",
    summary = "强制下线",
    params(("token" = String, Path, description = "session token id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn force_logout(
    State(state): State<AppState>,
    Path(token): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::force_logout(&state, &token).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list).layer(require_permission!("monitor:online:list")))
        .routes(routes!(force_logout).map(|r| {
            r.layer::<_, Infallible>(require_permission!("monitor:online:remove"))
                .layer(operlog!("在线用户", Delete))
        }))
}
