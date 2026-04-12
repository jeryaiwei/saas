//! Dept HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::ApiResponse;
use framework::{require_authenticated, require_permission};

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

pub fn router() -> Router<AppState> {
    Router::new()
        // CRITICAL: literal-prefix routes MUST be registered BEFORE wildcard `/{id}` routes.
        .route(
            "/system/dept/",
            post(create).route_layer(require_permission!("system:dept:add")),
        )
        .route(
            "/system/dept/",
            put(update).route_layer(require_permission!("system:dept:edit")),
        )
        .route(
            "/system/dept/list",
            get(list).route_layer(require_permission!("system:dept:list")),
        )
        .route(
            "/system/dept/option-select",
            get(option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/dept/list/exclude/{id}",
            get(exclude_list).route_layer(require_permission!("system:dept:exclude-list")),
        )
        .route(
            "/system/dept/{id}",
            get(find_by_id).route_layer(require_permission!("system:dept:query")),
        )
        .route(
            "/system/dept/{id}",
            delete(remove).route_layer(require_permission!("system:dept:remove")),
        )
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(create, update, list, option_select, exclude_list, find_by_id, remove))]
pub struct DeptApi;
