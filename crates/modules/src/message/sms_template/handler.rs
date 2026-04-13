//! SMS template HTTP handlers + router wiring.

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

#[utoipa::path(post, path = "/message/sms-template/", tag = "短信模板",
    summary = "新增短信模板",
    request_body = dto::CreateSmsTemplateDto,
    responses((status = 200, body = ApiResponse<dto::SmsTemplateResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateSmsTemplateDto>,
) -> Result<ApiResponse<dto::SmsTemplateResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/sms-template/list", tag = "短信模板",
    summary = "短信模板列表",
    params(dto::ListSmsTemplateDto),
    responses((status = 200, body = ApiResponse<Page<dto::SmsTemplateResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListSmsTemplateDto>,
) -> Result<ApiResponse<Page<dto::SmsTemplateResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/sms-template/{id}", tag = "短信模板",
    summary = "查询短信模板详情",
    params(("id" = i32, Path, description = "template id")),
    responses((status = 200, body = ApiResponse<dto::SmsTemplateResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<dto::SmsTemplateResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/sms-template/", tag = "短信模板",
    summary = "修改短信模板",
    request_body = dto::UpdateSmsTemplateDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateSmsTemplateDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/sms-template/{id}", tag = "短信模板",
    summary = "删除短信模板",
    params(("id" = String, Path, description = "ids, comma separated")),
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
            r.layer::<_, Infallible>(require_permission!("message:sms-template:add"))
                .layer(operlog!("短信模板", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("message:sms-template:list")))
        .routes(routes!(find_by_id).layer(require_permission!("message:sms-template:query")))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-template:edit"))
                .layer(operlog!("短信模板", Update))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-template:remove"))
                .layer(operlog!("短信模板", Delete))
        }))
}
