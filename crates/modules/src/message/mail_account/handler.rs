//! Mail account HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{operlog, require_permission};
use std::convert::Infallible;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/message/mail-account/", tag = "邮箱账号",
    summary = "新增邮箱账号",
    request_body = dto::CreateMailAccountDto,
    responses((status = 200, body = ApiResponse<dto::MailAccountResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateMailAccountDto>,
) -> Result<ApiResponse<dto::MailAccountResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/mail-account/list", tag = "邮箱账号",
    summary = "邮箱账号列表",
    params(dto::ListMailAccountDto),
    responses((status = 200, body = ApiResponse<Page<dto::MailAccountResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListMailAccountDto>,
) -> Result<ApiResponse<Page<dto::MailAccountResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/mail-account/enabled", tag = "邮箱账号",
    summary = "启用邮箱账号下拉",
    responses((status = 200, body = ApiResponse<Vec<dto::MailAccountOptionDto>>))
)]
pub(crate) async fn enabled(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::MailAccountOptionDto>>, AppError> {
    let resp = service::enabled_list(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/mail-account/{id}", tag = "邮箱账号",
    summary = "查询邮箱账号详情",
    params(("id" = i32, Path, description = "account id")),
    responses((status = 200, body = ApiResponse<dto::MailAccountResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<dto::MailAccountResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/mail-account/", tag = "邮箱账号",
    summary = "修改邮箱账号",
    request_body = dto::UpdateMailAccountDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateMailAccountDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/mail-account/{id}", tag = "邮箱账号",
    summary = "删除邮箱账号",
    params(("id" = i32, Path, description = "account id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, id).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(create).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-account:add"))
                .layer(operlog!("邮箱账号", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("message:mail-account:list")))
        .routes(routes!(enabled).layer(require_permission!("message:mail-account:list")))
        .routes(routes!(find_by_id).layer(require_permission!("message:mail-account:query")))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-account:edit"))
                .layer(operlog!("邮箱账号", Update))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-account:remove"))
                .layer(operlog!("邮箱账号", Delete))
        }))
}
