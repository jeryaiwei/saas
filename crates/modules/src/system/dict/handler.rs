//! Dict HTTP handlers + router wiring.
//!
//! Routes:
//!   /system/dict/type/*   — DictType CRUD
//!   /system/dict/data/*   — DictData CRUD

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

pub fn router() -> Router<AppState> {
    Router::new()
        // --- DictType routes ---
        .route(
            "/system/dict/type",
            post(create_type).route_layer(require_permission!("system:dict:add")),
        )
        .route(
            "/system/dict/type",
            put(update_type).route_layer(require_permission!("system:dict:edit")),
        )
        .route(
            "/system/dict/type/list",
            get(list_types).route_layer(require_permission!("system:dict:list")),
        )
        .route(
            "/system/dict/type/option-select",
            get(type_option_select).route_layer(require_authenticated!()),
        )
        .route(
            "/system/dict/type/{id}",
            get(find_type_by_id).route_layer(require_authenticated!()),
        )
        .route(
            "/system/dict/type/{id}",
            delete(remove_types).route_layer(require_permission!("system:dict:remove")),
        )
        // --- DictData routes ---
        .route(
            "/system/dict/data",
            post(create_data).route_layer(require_permission!("system:dict-data:add")),
        )
        .route(
            "/system/dict/data",
            put(update_data).route_layer(require_permission!("system:dict-data:edit")),
        )
        .route(
            "/system/dict/data/list",
            get(list_data).route_layer(require_permission!("system:dict-data:list")),
        )
        // literal-prefix routes BEFORE wildcard
        .route(
            "/system/dict/data/type/{dict_type}",
            get(find_data_by_type).route_layer(require_authenticated!()),
        )
        .route(
            "/system/dict/data/{id}",
            get(find_data_by_id).route_layer(require_authenticated!()),
        )
        .route(
            "/system/dict/data/{id}",
            delete(remove_data).route_layer(require_permission!("system:dict-data:remove")),
        )
}

#[derive(utoipa::OpenApi)]
#[openapi(paths(
    create_type,
    update_type,
    list_types,
    type_option_select,
    find_type_by_id,
    remove_types,
    create_data,
    update_data,
    list_data,
    find_data_by_id,
    find_data_by_type,
    remove_data
))]
pub struct DictApi;
