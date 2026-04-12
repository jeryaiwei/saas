//! Tenant Package HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission};

#[utoipa::path(get, path = "/system/tenant-package/{id}", tag = "套餐管理",
    summary = "查询套餐详情",
    params(("id" = String, Path, description = "package ID")),
    responses((status = 200, body = ApiResponse<dto::PackageDetailResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(package_id): Path<String>,
) -> Result<ApiResponse<dto::PackageDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &package_id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-package/list", tag = "套餐管理",
    summary = "套餐列表",
    params(dto::ListPackageDto),
    responses((status = 200, body = ApiResponse<Page<dto::PackageListItemResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListPackageDto>,
) -> Result<ApiResponse<Page<dto::PackageListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant-package/option-select", tag = "套餐管理",
    summary = "套餐下拉选项",
    responses((status = 200, body = ApiResponse<Vec<dto::PackageOptionResponseDto>>))
)]
pub(crate) async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::PackageOptionResponseDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/system/tenant-package/", tag = "套餐管理",
    summary = "新增套餐",
    request_body = dto::CreatePackageDto,
    responses((status = 200, body = ApiResponse<dto::PackageDetailResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreatePackageDto>,
) -> Result<ApiResponse<dto::PackageDetailResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/tenant-package/", tag = "套餐管理",
    summary = "修改套餐",
    request_body = dto::UpdatePackageDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdatePackageDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/system/tenant-package/{id}", tag = "套餐管理",
    summary = "删除套餐",
    params(("id" = String, Path, description = "package IDs (comma-separated)")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/tenant-package/",
            post(create).route_layer(require_permission!("system:tenant-package:add")),
        )
        .route(
            "/system/tenant-package/",
            put(update).route_layer(require_permission!("system:tenant-package:edit")),
        )
        .route(
            "/system/tenant-package/list",
            get(list).route_layer(require_permission!("system:tenant-package:list")),
        )
        .route(
            "/system/tenant-package/option-select",
            get(option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/tenant-package/{id}",
            get(find_by_id).route_layer(require_permission!("system:tenant-package:query")),
        )
        .route(
            "/system/tenant-package/{id}",
            delete(remove).route_layer(require_permission!("system:tenant-package:remove")),
        )
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(find_by_id, list, option_select, create, update, remove))]
pub struct TenantPackageApi;
