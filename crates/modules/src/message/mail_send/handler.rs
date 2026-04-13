//! Mail send HTTP handlers + router wiring.

use super::{dto, service};
use crate::state::AppState;
use axum::extract::{Path, State};
use framework::error::AppError;
use framework::extractors::ValidatedJson;
use framework::response::ApiResponse;
use framework::{operlog, require_permission};
use std::convert::Infallible;
use utoipa_axum::router::{OpenApiRouter, UtoipaMethodRouterExt};
use utoipa_axum::routes;

#[utoipa::path(post, path = "/message/mail-send", tag = "邮件发送",
    summary = "发送邮件",
    request_body = dto::SendMailDto,
    responses((status = 200, body = ApiResponse<dto::SendMailResponseDto>))
)]
pub(crate) async fn send(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::SendMailDto>,
) -> Result<ApiResponse<dto::SendMailResponseDto>, AppError> {
    let resp = service::send(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/message/mail-send/batch", tag = "邮件发送",
    summary = "批量发送邮件",
    request_body = dto::BatchSendMailDto,
    responses((status = 200, body = ApiResponse<dto::BatchSendMailResponseDto>))
)]
pub(crate) async fn batch_send(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::BatchSendMailDto>,
) -> Result<ApiResponse<dto::BatchSendMailResponseDto>, AppError> {
    let resp = service::batch_send(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/message/mail-send/resend/{logId}", tag = "邮件发送",
    summary = "重发失败邮件",
    params(("logId" = i64, Path, description = "mail log id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn resend(
    State(state): State<AppState>,
    Path(log_id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    service::resend(&state, log_id).await?;
    Ok(ApiResponse::success())
}

#[utoipa::path(post, path = "/message/mail-send/test", tag = "邮件发送",
    summary = "测试发送邮件",
    request_body = dto::TestMailDto,
    responses((status = 200, description = "success"))
)]
pub(crate) async fn test_send(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::TestMailDto>,
) -> Result<ApiResponse<()>, AppError> {
    service::test_send(&state, dto).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-send:send"))
                .layer(operlog!("邮件发送", Other))
        }))
        .routes(routes!(batch_send).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-send:batch"))
                .layer(operlog!("邮件发送", Other))
        }))
        .routes(routes!(resend).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-send:resend"))
                .layer(operlog!("邮件发送", Other))
        }))
        .routes(routes!(test_send).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:mail-account:test"))
                .layer(operlog!("邮件发送", Other))
        }))
}
