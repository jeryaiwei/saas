//! Mail template HTTP handlers + router wiring.

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

#[utoipa::path(post, path = "/message/mail-template/", tag = "邮件模板",
    summary = "新增邮件模板",
    request_body = dto::CreateMailTemplateDto,
    responses((status = 200, body = ApiResponse<dto::MailTemplateResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateMailTemplateDto>,
) -> Result<ApiResponse<dto::MailTemplateResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/mail-template/list", tag = "邮件模板",
    summary = "邮件模板列表",
    params(dto::ListMailTemplateDto),
    responses((status = 200, body = ApiResponse<Page<dto::MailTemplateResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListMailTemplateDto>,
) -> Result<ApiResponse<Page<dto::MailTemplateResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/mail-template/{id}", tag = "邮件模板",
    summary = "查询邮件模板详情",
    params(("id" = i32, Path, description = "template id")),
    responses((status = 200, body = ApiResponse<dto::MailTemplateResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<dto::MailTemplateResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/mail-template/", tag = "邮件模板",
    summary = "修改邮件模板",
    request_body = dto::UpdateMailTemplateDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateMailTemplateDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/mail-template/{id}", tag = "邮件模板",
    summary = "删除邮件模板",
    params(("id" = i32, Path, description = "template id")),
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
            r.layer::<_, Infallible>(require_permission!("message:mail-template:add"))
                .layer(operlog!("邮件模板", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("message:mail-template:list")))
        .routes(routes!(find_by_id).layer(require_permission!("message:mail-template:query")))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-template:edit"))
                .layer(operlog!("邮件模板", Update))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-template:remove"))
                .layer(operlog!("邮件模板", Delete))
        }))
}
