//! SMS send HTTP handlers + router wiring.

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

#[utoipa::path(post, path = "/message/sms-send", tag = "短信发送",
    summary = "发送短信",
    request_body = dto::SendSmsDto,
    responses((status = 200, body = ApiResponse<dto::SendSmsResponseDto>))
)]
pub(crate) async fn send(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::SendSmsDto>,
) -> Result<ApiResponse<dto::SendSmsResponseDto>, AppError> {
    let resp = service::send(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/message/sms-send/batch", tag = "短信发送",
    summary = "批量发送短信",
    request_body = dto::BatchSendSmsDto,
    responses((status = 200, body = ApiResponse<dto::BatchSendSmsResponseDto>))
)]
pub(crate) async fn batch_send(
    State(state): State<AppState>,
    ValidatedJson(dto): ValidatedJson<dto::BatchSendSmsDto>,
) -> Result<ApiResponse<dto::BatchSendSmsResponseDto>, AppError> {
    let resp = service::batch_send(&state, dto).await?;
    Ok(ApiResponse::ok(resp))
}

#[utoipa::path(post, path = "/message/sms-send/resend/{logId}", tag = "短信发送",
    summary = "重发失败短信",
    params(("logId" = i64, Path, description = "sms log id")),
    responses((status = 200, description = "success"))
)]
pub(crate) async fn resend(
    State(state): State<AppState>,
    Path(log_id): Path<i64>,
) -> Result<ApiResponse<()>, AppError> {
    service::resend(&state, log_id).await?;
    Ok(ApiResponse::success())
}

pub fn router() -> OpenApiRouter<AppState> {
    OpenApiRouter::new()
        .routes(routes!(send).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-send:send"))
                .layer(operlog!("短信发送", Other))
        }))
        .routes(routes!(batch_send).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-send:batch"))
                .layer(operlog!("短信发送", Other))
        }))
        .routes(routes!(resend).map(|r| {
            r.layer::<_, Infallible>(require_permission!("message:sms-send:resend"))
                .layer(operlog!("短信发送", Other))
        }))
}
