//! Notice HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::require_permission;
use framework::response::{ApiResponse, Page};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/message/notice/", tag = "通知公告",
    summary = "新增通知",
    request_body = dto::CreateNoticeDto,
    responses((status = 200, body = ApiResponse<dto::NoticeResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateNoticeDto>,
) -> Result<ApiResponse<dto::NoticeResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/notice/", tag = "通知公告",
    summary = "修改通知",
    request_body = dto::UpdateNoticeDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateNoticeDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/message/notice/list", tag = "通知公告",
    summary = "通知列表",
    params(dto::ListNoticeDto),
    responses((status = 200, body = ApiResponse<Page<dto::NoticeResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListNoticeDto>,
) -> Result<ApiResponse<Page<dto::NoticeResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/notice/{id}", tag = "通知公告",
    summary = "查询通知详情",
    params(("id" = String, Path, description = "notice id")),
    responses((status = 200, body = ApiResponse<dto::NoticeResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::NoticeResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/message/notice/{id}", tag = "通知公告",
    summary = "删除通知",
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
        .routes(routes!(create).layer(require_permission!("message:notice:add")))
        .routes(routes!(update).layer(require_permission!("message:notice:edit")))
        .routes(routes!(list).layer(require_permission!("message:notice:list")))
        .routes(routes!(find_by_id).layer(require_permission!("message:notice:query")))
        .routes(routes!(remove).layer(require_permission!("message:notice:remove")))
}
