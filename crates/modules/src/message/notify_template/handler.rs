//! Notify template HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::{ValidatedJson, ValidatedQuery};
use framework::response::{ApiResponse, Page};
use framework::{operlog, require_authenticated, require_permission};
use std::convert::Infallible;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/message/notify-template/", tag = "站内信模板",
    summary = "新增站内信模板",
    request_body = dto::CreateNotifyTemplateDto,
    responses((status = 200, body = ApiResponse<dto::NotifyTemplateResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateNotifyTemplateDto>,
) -> Result<ApiResponse<dto::NotifyTemplateResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/notify-template/list", tag = "站内信模板",
    summary = "站内信模板列表",
    params(dto::ListNotifyTemplateDto),
    responses((status = 200, body = ApiResponse<Page<dto::NotifyTemplateResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListNotifyTemplateDto>,
) -> Result<ApiResponse<Page<dto::NotifyTemplateResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/notify-template/select", tag = "站内信模板",
    summary = "站内信模板下拉",
    responses((status = 200, body = ApiResponse<Vec<dto::NotifyTemplateOptionDto>>))
)]
pub(crate) async fn option_select(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::NotifyTemplateOptionDto>>, AppError> {
    let resp = service::option_select(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/notify-template/{id}", tag = "站内信模板",
    summary = "查询站内信模板详情",
    params(("id" = i32, Path, description = "template id")),
    responses((status = 200, body = ApiResponse<dto::NotifyTemplateResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<dto::NotifyTemplateResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/notify-template/", tag = "站内信模板",
    summary = "修改站内信模板",
    request_body = dto::UpdateNotifyTemplateDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateNotifyTemplateDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/notify-template/{id}", tag = "站内信模板",
    summary = "删除站内信模板",
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
            r.layer::<_, Infallible>(require_permission!("message:notify-template:add"))
                .layer(operlog!("站内信模板", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("message:notify-template:list")))
        .routes(routes!(option_select).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_permission!("message:notify-template:query")))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:notify-template:edit"))
                .layer(operlog!("站内信模板", Update))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:notify-template:remove"))
                .layer(operlog!("站内信模板", Delete))
        }))
}
