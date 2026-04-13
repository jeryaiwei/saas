//! Notify message HTTP handlers + router wiring.

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

#[utoipa::path(post, path = "/message/notify-message/send", tag = "站内信消息",
    summary = "发送站内信",
    request_body = dto::SendNotifyMessageDto,
    responses((status = 200, body = ApiResponse<dto::NotifyMessageResponseDto>))
)]
pub(crate) async fn send(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::SendNotifyMessageDto>,
) -> Result<ApiResponse<dto::NotifyMessageResponseDto>, AppError> {
    let resp = service::send(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/message/notify-message/send-all", tag = "站内信消息",
    summary = "全员发送站内信",
    request_body = dto::SendAllNotifyMessageDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn send_all(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::SendAllNotifyMessageDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::send_all(&state, dto).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(get, path = "/message/notify-message/list", tag = "站内信消息",
    summary = "站内信列表",
    params(dto::ListNotifyMessageDto),
    responses((status = 200, body = ApiResponse<Page<dto::NotifyMessageResponseDto>>))
)]
pub(crate) async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::ListNotifyMessageDto>,
) -> Result<ApiResponse<Page<dto::NotifyMessageResponseDto>>, AppError> {
    let page = service::list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/notify-message/my-list", tag = "站内信消息",
    summary = "我的站内信",
    params(dto::MyNotifyMessageDto),
    responses((status = 200, body = ApiResponse<Page<dto::NotifyMessageResponseDto>>))
)]
pub(crate) async fn my_list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<dto::MyNotifyMessageDto>,
) -> Result<ApiResponse<Page<dto::NotifyMessageResponseDto>>, AppError> {
    let page = service::my_list(&state, query).await?;
    Ok(ApiResponse::ok(page))
}

#[utoipa::path(get, path = "/message/notify-message/unread-count", tag = "站内信消息",
    summary = "未读消息数",
    responses((status = 200, body = ApiResponse<dto::UnreadCountDto>))
)]
pub(crate) async fn unread_count(
    State(state): State<AppState>,
) -> Result<ApiResponse<dto::UnreadCountDto>, AppError> {
    let resp = service::unread_count(&state).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(get, path = "/message/notify-message/{id}", tag = "站内信消息",
    summary = "查询站内信详情",
    params(("id" = i64, Path, description = "message id")),
    responses((status = 200, body = ApiResponse<dto::NotifyMessageResponseDto>))
)]
pub(crate) async fn find_by_id(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<dto::NotifyMessageResponseDto>, AppError> {
    let resp = service::find_by_id(&state, id).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(put, path = "/message/notify-message/read/{id}", tag = "站内信消息",
    summary = "标记已读",
    params(("id" = i64, Path, description = "message id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn mark_read(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    service::mark_read(&state, id).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/message/notify-message/read-batch/{ids}", tag = "站内信消息",
    summary = "批量标记已读",
    params(("ids" = String, Path, description = "ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn mark_read_batch(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::mark_read_batch(&state, &ids).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(put, path = "/message/notify-message/read-all", tag = "站内信消息",
    summary = "全部标记已读",
    responses((status = 200, description = "success"))
)]
pub(crate) async fn mark_all_read(
    State(state): State<AppState>,
) -> Result<ApiResponse<()>, AppError> {
    service::mark_all_read(&state).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/notify-message/{id}", tag = "站内信消息",
    summary = "删除站内信",
    params(("id" = i64, Path, description = "message id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove(&state, id).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(delete, path = "/message/notify-message/batch/{ids}", tag = "站内信消息",
    summary = "批量删除站内信",
    params(("ids" = String, Path, description = "ids, comma separated")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn remove_batch(
    State(state): State<AppState>,
    Path(ids): Path<String>,
) -> Result<ApiResponse<()>, AppError> {
    service::remove_batch(&state, &ids).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:notify-message:send"))
                .layer(operlog!("站内信", Insert))
        }))
        .routes(routes!(send_all).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:notify-message:send-all"))
                .layer(operlog!("站内信", Insert))
        }))
        .routes(routes!(list).layer(require_permission!("message:notify-message:list")))
        .routes(routes!(my_list).layer(require_authenticated!()))
        .routes(routes!(unread_count).layer(require_authenticated!()))
        .routes(routes!(find_by_id).layer(require_authenticated!()))
        .routes(routes!(mark_read).map(|r| {
            r.layer::<_, Infallible>(require_authenticated!())
                .layer(operlog!("站内信", Update))
        }))
        .routes(routes!(mark_read_batch).map(|r| {
            r.layer::<_, Infallible>(require_authenticated!())
                .layer(operlog!("站内信", Update))
        }))
        .routes(routes!(mark_all_read).map(|r| {
            r.layer::<_, Infallible>(require_authenticated!())
                .layer(operlog!("站内信", Update))
        }))
        .routes(routes!(remove).map(|r| {
            r.layer::<_, Infallible>(require_authenticated!())
                .layer(operlog!("站内信", Delete))
        }))
        .routes(routes!(remove_batch).map(|r| {
            r.layer::<_, Infallible>(require_authenticated!())
                .layer(operlog!("站内信", Delete))
        }))
}
