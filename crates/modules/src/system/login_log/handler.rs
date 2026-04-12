//! LoginLog HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::ValidatedQuery;
use framework::operlog;
use framework::require_permission;
use framework::response::{ApiResponse, Page};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/monitor/logininfor/list", tag = "登录日志",
    summary = "登录日志列表",
    params(dto::ListLoginLogDto),
    responses((status = 200, body = ApiResponse<Page<dto::LoginLogResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListLoginLogDto>,
) -> Result<ApiResponse<Page<dto::LoginLogResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(delete, path = "/monitor/logininfor/clean", tag = "登录日志",
    summary = "清空登录日志",
    responses((status = 200, description = "success"))
)]
pub(crate) async fn clean(State(state): State<AppState>) -> Result<ApiResponse<()>, AppError> {
    service::clean(&state).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/monitor/logininfor/{id}", tag = "登录日志",
    summary = "删除登录日志",
    params(("id" = String, Path, description = "ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list).layer(require_permission!("monitor:logininfor:list")))
        // literal-prefix routes BEFORE wildcard `/{id}`
        .routes(routes!(clean).map(|r| {
            r.layer::<_, Infallible>(require_permission!("monitor:logininfor:clean"))
                .layer(operlog!("登录日志", Clean))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("monitor:logininfor:remove"))
                .layer(operlog!("登录日志", Delete))
        }))
}
