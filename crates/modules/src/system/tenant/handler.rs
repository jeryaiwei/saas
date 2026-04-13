//! Tenant HTTP handlers + router wiring.

use std::convert::Infallible;

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Extension, Path, State};
use framework::auth::{JwtClaims, Role};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{operlog, require_access, require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/system/tenant/", tag = "租户管理",
    summary = "新增租户",
    request_body = dto::CreateTenantDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateTenantDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::create(&state, dto).await
}

#[utoipa::path(put, path = "/system/tenant/", tag = "租户管理",
    summary = "修改租户",
    request_body = dto::UpdateTenantDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateTenantDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/tenant/list", tag = "租户管理",
    summary = "租户列表",
    params(dto::ListTenantDto),
    responses((status = 200, body = ApiResponse<Page<dto::TenantListItemResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListTenantDto>,
) -> Result<ApiResponse<Page<dto::TenantListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant/{id}", tag = "租户管理",
    summary = "查询租户详情",
    params(("id" = String, Path, description = "tenant id")),
    responses((status = 200, body = ApiResponse<dto::TenantDetailResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::TenantDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/tenant/{id}", tag = "租户管理",
    summary = "删除租户",
    params(("id" = String, Path, description = "tenant ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/tenant/select-list", tag = "租户管理",
    summary = "租户下拉列表",
    responses((status = 200, body = ApiResponse<Vec<dto::TenantSelectOptionDto>>))
)]
pub(crate) async fn select_list(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::TenantSelectOptionDto>>, AppError> {
    let resp = service::select_list(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant/dynamic/{tenantId}", tag = "租户管理",
    summary = "切换租户",
    params(("tenantId" = String, Path, description = "target tenant id")),
    responses((status = 200, body = ApiResponse<crate::auth::dto::LoginTokenResponseDto>))
)]
pub(crate) async fn dynamic_switch(
    State(state): State<AppState>,
    Path(tenant_id): Path<String>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<ApiResponse<crate::auth::dto::LoginTokenResponseDto>, AppError> {
    let resp = service::dynamic_switch(&state, &tenant_id, &claims).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant/dynamic/clear", tag = "租户管理",
    summary = "恢复默认租户",
    responses((status = 200, body = ApiResponse<crate::auth::dto::LoginTokenResponseDto>))
)]
pub(crate) async fn dynamic_clear(
    State(state): State<AppState>,
    Extension(claims): Extension<JwtClaims>,
) -> Result<ApiResponse<crate::auth::dto::LoginTokenResponseDto>, AppError> {
    let resp = service::dynamic_clear(&state, &claims).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/tenant/switch-status", tag = "租户管理",
    summary = "租户切换状态",
    responses((status = 200, body = ApiResponse<dto::TenantSwitchStatusDto>))
)]
pub(crate) async fn switch_status(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::TenantSwitchStatusDto>, AppError> {
    let resp = service::switch_status(&state).await?;
    Ok(ApiResponse::ok(resp))
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_access! {
                permission: "system:tenant:add",
                role: Role::PlatformAdmin,
            })
            .layer(operlog!("租户管理", Insert))
        }))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("system:tenant:edit"))
                .layer(operlog!("租户管理", Update))
        }))
        .routes(routes!(list).layer(require_access! {
            permission: "system:tenant:list",
            role: Role::SuperAdmin,
        }))
        .routes(routes!(select_list).layer(require_authenticated!()))
        // literal-prefix routes BEFORE wildcard `/{tenantId}`
        .routes(routes!(dynamic_clear).layer(require_authenticated!()))
        .routes(routes!(switch_status).layer(require_authenticated!()))
        .routes(routes!(dynamic_switch).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_permission!("system:tenant:query")))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_access! {
                permission: "system:tenant:remove",
                role: Role::PlatformAdmin,
            })
            .layer(operlog!("租户管理", Delete))
        }))
}
