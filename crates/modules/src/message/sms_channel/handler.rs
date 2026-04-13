//! SMS channel HTTP handlers + router wiring.

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

#[utoipa::path(post, path = "/message/sms-channel/", tag = "短信渠道",
    summary = "新增短信渠道",
    request_body = dto::CreateSmsChannelDto,
    responses((status = 200, body = ApiResponse<dto::SmsChannelResponseDto>))
)]
pub(crate) async fn create(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::CreateSmsChannelDto>,
) -> Result<ApiResponse<dto::SmsChannelResponseDto>, AppError> {
    let resp = service::create(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/sms-channel/list", tag = "短信渠道",
    summary = "短信渠道列表",
    params(dto::ListSmsChannelDto),
    responses((status = 200, body = ApiResponse<Page<dto::SmsChannelResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListSmsChannelDto>,
) -> Result<ApiResponse<Page<dto::SmsChannelResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/sms-channel/enabled", tag = "短信渠道",
    summary = "启用渠道下拉",
    responses((status = 200, body = ApiResponse<Vec<dto::SmsChannelOptionDto>>))
)]
pub(crate) async fn enabled(
    State(state): State<AppState>,
) -> Result<ApiResponse<Vec<dto::SmsChannelOptionDto>>, AppError> {
    let resp = service::enabled_list(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/sms-channel/{id}", tag = "短信渠道",
    summary = "查询短信渠道详情",
    params(("id" = i32, Path, description = "channel id")),
    responses((status = 200, body = ApiResponse<dto::SmsChannelResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<ApiResponse<dto::SmsChannelResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/sms-channel/", tag = "短信渠道",
    summary = "修改短信渠道",
    request_body = dto::UpdateSmsChannelDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn update(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::UpdateSmsChannelDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::update(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/sms-channel/{id}", tag = "短信渠道",
    summary = "删除短信渠道",
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
            r.layer::<_, Infallible>(require_permission!("message:sms-channel:add"))
                .layer(operlog!("短信渠道", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("message:sms-channel:list")))
        .routes(routes!(enabled).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_permission!("message:sms-channel:query")))
        .routes(routes!(update).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-channel:edit"))
                .layer(operlog!("短信渠道", Update))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-channel:remove"))
                .layer(operlog!("短信渠道", Delete))
        }))
}
