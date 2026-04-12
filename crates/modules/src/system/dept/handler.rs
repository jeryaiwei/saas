//! Dept HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::operlog;
use framework::response::ApiResponse;
use framework::{require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/system/dept/", tag = "部门管理",
    summary = "新增部门",
    request_body = dto::CreateDeptDto,
    responses((status = 200, body = ApiResponse<dto::DeptResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateDeptDto>,
) -> Result<ApiResponse<dto::DeptResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/dept/", tag = "部门管理",
    summary = "修改部门",
    request_body = dto::UpdateDeptDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateDeptDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/dept/list", tag = "部门管理",
    summary = "部门列表",
    params(dto::ListDeptDto),
    responses((status = 200, body = ApiResponse<Vec<dto::DeptResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListDeptDto>,
) -> Result<ApiResponse<Vec<dto::DeptResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/dept/option-select", tag = "部门管理",
    summary = "部门下拉选项",
    responses((status = 200, body = ApiResponse<Vec<dto::DeptResponseDto>>))
)]
pub(crate) async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::DeptResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/dept/list/exclude/{id}", tag = "部门管理",
    summary = "部门列表（排除指定节点）",
    params(("id" = String, Path, description = "dept id to exclude")),
    responses((status = 200, body = ApiResponse<Vec<dto::DeptResponseDto>>))
)]
pub(crate) async fn exclude_list(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<Vec<dto::DeptResponseDto>>, AppError> {
    let resp = service::exclude_list(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/dept/{id}", tag = "部门管理",
    summary = "查询部门详情",
    params(("id" = String, Path, description = "dept id")),
    responses((status = 200, body = ApiResponse<dto::DeptResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::DeptResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/dept/{id}", tag = "部门管理",
    summary = "删除部门",
    params(("id" = String, Path, description = "dept id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &id).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:dept:add"))
                .layer(operlog!("部门管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:dept:edit"))
                .layer(operlog!("部门管理", Update))
        }))
        .routes(routes!(list).layer(require_permission!("system:dept:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(exclude_list).layer(require_permission!("system:dept:exclude-list")))
        .routes(routes!(find_by_id).layer(require_permission!("system:dept:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:dept:remove"))
                .layer(operlog!("部门管理", Delete))
        }))
}
