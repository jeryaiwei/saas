//! Dict HTTP handlers + router wiring.
//!
//! Routes:
//!   /system/dict/type/*   — DictType CRUD
//!   /system/dict/data/*   — DictData CRUD

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{require_authenticated, require_permission};
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

// ---------------------------------------------------------------------------
// DictType handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post, path = "/system/dict/type", tag = "字典管理",
    summary = "新增字典类型",
    request_body = dto::CreateDictTypeDto,
    responses((status = 200, body = ApiResponse<dto::DictTypeResponseDto>))
)]
pub(crate) async fn create_type(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateDictTypeDto>,
) -> Result<ApiResponse<dto::DictTypeResponseDto>, AppError> {
    let resp = service::create_type(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/dict/type", tag = "字典管理",
    summary = "修改字典类型",
    request_body = dto::UpdateDictTypeDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_type(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateDictTypeDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_type(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/dict/type/list", tag = "字典管理",
    summary = "字典类型列表",
    params(dto::ListDictTypeDto),
    responses((status = 200, body = ApiResponse<Page<dto::DictTypeResponseDto>>))
)]
pub(crate) async fn list_types(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListDictTypeDto>,
) -> Result<ApiResponse<Page<dto::DictTypeResponseDto>>, AppError> {
    let page = service::list_types(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/system/dict/type/option-select", tag = "字典管理",
    summary = "字典类型下拉选项",
    responses((status = 200, body = ApiResponse<Vec<dto::DictTypeResponseDto>>))
)]
pub(crate) async fn type_option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::DictTypeResponseDto>>, AppError> {
    let resp = service::type_option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/dict/type/{id}", tag = "字典管理",
    summary = "查询字典类型详情",
    params(("id" = String, Path, description = "dict type ID")),
    responses((status = 200, body = ApiResponse<dto::DictTypeResponseDto>))
)]
pub(crate) async fn find_type_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::DictTypeResponseDto>, AppError> {
    let resp = service::find_type_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/dict/type/{id}", tag = "字典管理",
    summary = "删除字典类型",
    params(("id" = String, Path, description = "dict type IDs (comma-separated)")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove_types(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove_types(&state, &ids).await?;
    Ok(ApiResponse::success())
}

// ---------------------------------------------------------------------------
// DictData handlers
// ---------------------------------------------------------------------------

#[utoipa::path(post, path = "/system/dict/data", tag = "字典管理",
    summary = "新增字典数据",
    request_body = dto::CreateDictDataDto,
    responses((status = 200, body = ApiResponse<dto::DictDataResponseDto>))
)]
pub(crate) async fn create_data(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateDictDataDto>,
) -> Result<ApiResponse<dto::DictDataResponseDto>, AppError> {
    let resp = service::create_data(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/system/dict/data", tag = "字典管理",
    summary = "修改字典数据",
    request_body = dto::UpdateDictDataDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update_data(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateDictDataDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update_data(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/system/dict/data/list", tag = "字典管理",
    summary = "字典数据列表",
    params(dto::ListDictDataDto),
    responses((status = 200, body = ApiResponse<Page<dto::DictDataResponseDto>>))
)]
pub(crate) async fn list_data(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListDictDataDto>,
) -> Result<ApiResponse<Page<dto::DictDataResponseDto>>, AppError> {
    let page = service::list_data(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/system/dict/data/{id}", tag = "字典管理",
    summary = "查询字典数据详情",
    params(("id" = String, Path, description = "dict data ID")),
    responses((status = 200, body = ApiResponse<dto::DictDataResponseDto>))
)]
pub(crate) async fn find_data_by_id(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<ApiResponse<dto::DictDataResponseDto>, AppError> {
    let resp = service::find_data_by_id(&state, &id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/system/dict/data/type/{dict_type}", tag = "字典管理",
    summary = "按类型查询字典数据",
    params(("dict_type" = String, Path, description = "dict type name")),
    responses((status = 200, body = ApiResponse<Vec<dto::DictDataResponseDto>>))
)]
pub(crate) async fn find_data_by_type(
    State(state): State<AppState>,
    Path(dict_type): Path<String>,
) -> Result<ApiResponse<Vec<dto::DictDataResponseDto>>, AppError> {
    let resp = service::find_data_by_type(&state, &dict_type).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(delete, path = "/system/dict/data/{id}", tag = "字典管理",
    summary = "删除字典数据",
    params(("id" = String, Path, description = "dict data IDs (comma-separated)")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove_data(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove_data(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        // --- DictType routes ---
        .routes(routes!(create_type).layer(require_permission!("system:dict:add")))
        .routes(routes!(update_type).layer(require_permission!("system:dict:edit")))
        .routes(routes!(list_types).layer(require_permission!("system:dict:list")))
        .routes(routes!(type_option_select).layer(require_authenticated!()))
        .routes(routes!(find_type_by_id).layer(require_authenticated!()))
        .routes(routes!(remove_types).layer(require_permission!("system:dict:remove")))
        // --- DictData routes ---
        .routes(routes!(create_data).layer(require_permission!("system:dict-data:add")))
        .routes(routes!(update_data).layer(require_permission!("system:dict-data:edit")))
        .routes(routes!(list_data).layer(require_permission!("system:dict-data:list")))
        .routes(routes!(find_data_by_type).layer(require_authenticated!()))
        .routes(routes!(find_data_by_id).layer(require_authenticated!()))
        .routes(routes!(remove_data).layer(require_permission!("system:dict-data:remove")))
}
