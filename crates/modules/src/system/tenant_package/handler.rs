//! Tenant Package HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{operlog, require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

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

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:tenant-package:add"))
                .layer(operlog!("套餐管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:tenant-package:edit"))
                .layer(operlog!("套餐管理", Update))
        }))
        .routes(routes!(list).layer(require_permission!("system:tenant-package:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_permission!("system:tenant-package:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:tenant-package:remove"))
                .layer(operlog!("套餐管理", Delete))
        }))
}
