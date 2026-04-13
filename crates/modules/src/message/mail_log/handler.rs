//! Mail log HTTP handlers + router wiring (READ-ONLY).

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::ValidatedQuery;
use framework::require_permission;
use framework::response::{ApiResponse, Page};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(get, path = "/message/mail-log/list", tag = "邮件日志",
    summary = "邮件日志列表",
    params(dto::ListMailLogDto),
    responses((status = 200, body = ApiResponse<Page<dto::MailLogResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListMailLogDto>,
) -> Result<ApiResponse<Page<dto::MailLogResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/mail-log/{id}", tag = "邮件日志",
    summary = "查询邮件日志详情",
    params(("id" = i64, Path, description = "log id")),
    responses((status = 200, body = ApiResponse<dto::MailLogResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<dto::MailLogResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(list).layer(require_permission!("message:mail-log:list")))
        .routes(routes!(find_by_id).layer(require_permission!("message:mail-log:query")))
}
