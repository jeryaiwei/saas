//! Post HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/system/post/", tag = "岗位管理",
    summary = "新增岗位",
    request_body = dto::CreatePostDto,
    responses((status = 200, body = ApiResponse<dto::PostResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreatePostDto>,
) -> Result<ApiResponse<dto::PostResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/post/", tag = "岗位管理",
    summary = "修改岗位",
    request_body = dto::UpdatePostDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdatePostDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/post/list", tag = "岗位管理",
    summary = "岗位列表",
    params(dto::ListPostDto),
    responses((status = 200, body = ApiResponse<Page<dto::PostResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListPostDto>,
) -> Result<ApiResponse<Page<dto::PostResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/system/post/option-select", tag = "岗位管理",
    summary = "岗位下拉选项",
    responses((status = 200, body = ApiResponse<Vec<dto::PostResponseDto>>))
)]
pub(crate) async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::PostResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/post/{id}", tag = "岗位管理",
    summary = "查询岗位详情",
    params(("id" = String, Path, description = "post id")),
    responses((status = 200, body = ApiResponse<dto::PostResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::PostResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/post/{id}", tag = "岗位管理",
    summary = "删除岗位",
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
        .routes(routes!(create).layer(require_permission!("system:post:add")))
        .routes(routes!(update).layer(require_permission!("system:post:edit")))
        .routes(routes!(list).layer(require_permission!("system:post:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_permission!("system:post:query")))
        .routes(routes!(remove).layer(require_permission!("system:post:remove")))
}
