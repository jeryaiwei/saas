//! OperLog HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::ValidatedQuery;
use framework::require_permission;
use framework::response::{ApiResponse, Page};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/monitor/operlog/list", tag = "操作日志",
    summary = "操作日志列表",
    params(dto::ListOperLogDto),
    responses((status = 200, body = ApiResponse<Page<dto::OperLogResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListOperLogDto>,
) -> Result<ApiResponse<Page<dto::OperLogResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/monitor/operlog/{id}", tag = "操作日志",
    summary = "查询操作日志详情",
    params(("id" = String, Path, description = "oper log id")),
    responses((status = 200, body = ApiResponse<dto::OperLogResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::OperLogResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/monitor/operlog/clean", tag = "操作日志",
    summary = "清空操作日志",
    responses((status = 200, description = "success"))
)]
pub(crate) async fn clean(State(state): State<AppState>) -> Result<ApiResponse<()>, AppError> {
    service::clean(&state).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/monitor/operlog/{id}", tag = "操作日志",
    summary = "删除操作日志",
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
        .routes(routes!(list).layer(require_permission!("monitor:operlog:list")))
        // literal-prefix routes BEFORE wildcard `/{id}`
        .routes(routes!(clean).layer(require_permission!("monitor:operlog:clean")))
        .routes(routes!(find_by_id).layer(require_permission!("monitor:operlog:query")))
        .routes(routes!(remove).layer(require_permission!("monitor:operlog:remove")))
}
