//! Tenant HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::{
    extract::{Path, State},
    routing::{delete, get, post, put},
    Router,
};
use framework::auth::Role;
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_access, require_permission};

async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateTenantDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::create(&state, dto).await
}

async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateTenantDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListTenantDto>,
) -> Result<ApiResponse<Page<dto::TenantListItemResponseDto>>, AppError> {
    let resp = service::list(&state, query).await?;
    Ok(ApiResponse::ok(resp))
}

async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::TenantDetailResponseDto>, AppError> {
    let resp = service::find_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

async fn remove(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/system/tenant/",
            post(create).route_layer(require_access! {
                permission: "system:tenant:add",
                role: Role::PlatformAdmin,
            }),
        )
        .route(
            "/system/tenant/",
            put(update).route_layer(require_permission!("system:tenant:edit")),
        )
        .route(
            "/system/tenant/list",
            get(list).route_layer(require_access! {
                permission: "system:tenant:list",
                role: Role::SuperAdmin,
            }),
        )
        .route(
            "/system/tenant/{id}",
            get(find_by_id).route_layer(require_permission!("system:tenant:query")),
        )
        .route(
            "/system/tenant/{id}",
            delete(remove).route_layer(require_access! {
                permission: "system:tenant:remove",
                role: Role::PlatformAdmin,
            }),
        )
}
