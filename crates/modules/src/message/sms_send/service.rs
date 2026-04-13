//! SMS send service — business orchestration for sending SMS.

use super::client::{self, SmsClient, SmsSendParams};
use super::dto::*;
use crate::domain::{SmsChannelRepo, SmsLogInsertParams, SmsLogRepo, SmsTemplateRepo};
use crate::message::template_parser;
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::ResponseCode;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;

/// Send a single SMS.
#[tracing::instrument(skip_all, fields(mobile = %dto.mobile, template_code = %dto.template_code))]
pub async fn send(state: &AppState, dto: SendSmsDto) -> Result<SendSmsResponseDto, AppError> {
    // 1. Find enabled template
    let template = SmsTemplateRepo::find_enabled_by_code(&state.pg, &dto.template_code)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_TEMPLATE_NOT_FOUND)?;

    // 2. Find channel + check enabled
    let channel = SmsChannelRepo::find_by_id(&state.pg, template.channel_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_CHANNEL_NOT_FOUND)?;
    if channel.status != "0" {
        return Err(AppError::business_with_msg(
            ResponseCode::SMS_CHANNEL_NOT_FOUND,
            "SMS channel is disabled",
        ));
    }

    // 3. Validate + render template params
    let params = dto.params.unwrap_or_default();
    template_parser::validate_params(
        &template.content,
        &params,
        ResponseCode::SMS_TEMPLATE_PARAMS_MISSING,
    )?;
    let rendered_content = template_parser::render(&template.content, &params);

    // 4. Write sms_log
    let log = SmsLogRepo::insert(
        &state.pg,
        SmsLogInsertParams {
            channel_id: channel.id,
            channel_code: channel.code.clone(),
            template_id: template.id,
            template_code: template.code.clone(),
            mobile: dto.mobile.clone(),
            content: rendered_content.clone(),
            params: serde_json::to_string(&params).ok(),
        },
    )
    .await
    .into_internal()?;

    // 5. Create SMS client
    let sms_client: Arc<dyn SmsClient> =
        Arc::from(client::create_client(&channel.code, &channel.api_key, &channel.api_secret)?);

    // 6. Build send params and spawn background task
    let sms_params = SmsSendParams {
        mobile: dto.mobile,
        signature: channel.signature.clone(),
        api_template_id: template.api_template_id.clone(),
        params,
    };
    let pg = state.pg.clone();
    let sem = state.sms_semaphore.clone();
    let log_id = log.id;
    tokio::spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        execute_sms_send(&pg, log_id, sms_client, sms_params).await;
    });

    // 7. Return log id
    Ok(SendSmsResponseDto { log_id: log.id })
}

/// Send SMS to a batch of mobiles.
#[tracing::instrument(skip_all, fields(count = dto.mobiles.len(), template_code = %dto.template_code))]
pub async fn batch_send(
    state: &AppState,
    dto: BatchSendSmsDto,
) -> Result<BatchSendSmsResponseDto, AppError> {
    // 1. Validate batch size
    if dto.mobiles.len() > 100 {
        return Err(AppError::business(ResponseCode::BATCH_SIZE_EXCEEDED));
    }

    // 2. Find template + channel
    let template = SmsTemplateRepo::find_enabled_by_code(&state.pg, &dto.template_code)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_TEMPLATE_NOT_FOUND)?;

    let channel = SmsChannelRepo::find_by_id(&state.pg, template.channel_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_CHANNEL_NOT_FOUND)?;
    if channel.status != "0" {
        return Err(AppError::business_with_msg(
            ResponseCode::SMS_CHANNEL_NOT_FOUND,
            "SMS channel is disabled",
        ));
    }

    let params = dto.params.unwrap_or_default();
    template_parser::validate_params(
        &template.content,
        &params,
        ResponseCode::SMS_TEMPLATE_PARAMS_MISSING,
    )?;
    let rendered_content = template_parser::render(&template.content, &params);

    // 3. Create client once for the batch
    let sms_client: Arc<dyn SmsClient> =
        Arc::from(client::create_client(&channel.code, &channel.api_key, &channel.api_secret)?);
    let count = dto.mobiles.len() as i32;
    let params_json = serde_json::to_string(&params).ok();

    for mobile in dto.mobiles {
        let log = SmsLogRepo::insert(
            &state.pg,
            SmsLogInsertParams {
                channel_id: channel.id,
                channel_code: channel.code.clone(),
                template_id: template.id,
                template_code: template.code.clone(),
                mobile: mobile.clone(),
                content: rendered_content.clone(),
                params: params_json.clone(),
            },
        )
        .await
        .into_internal()?;

        let sms_params = SmsSendParams {
            mobile,
            signature: channel.signature.clone(),
            api_template_id: template.api_template_id.clone(),
            params: params.clone(),
        };
        let pg = state.pg.clone();
        let sem = state.sms_semaphore.clone();
        let log_id = log.id;
        let client = sms_client.clone();
        tokio::spawn(async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            execute_sms_send(&pg, log_id, client, sms_params).await;
        });
    }

    // 4. Return count
    Ok(BatchSendSmsResponseDto { count })
}

/// Resend a previously failed SMS.
#[tracing::instrument(skip_all, fields(log_id = %log_id))]
pub async fn resend(state: &AppState, log_id: i64) -> Result<(), AppError> {
    // 1. Find log
    let log = SmsLogRepo::find_by_id(&state.pg, log_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SEND_LOG_NOT_FOUND)?;

    // 2. Check send_status == 2 (FAILED)
    if log.send_status != 2 {
        return Err(AppError::business(ResponseCode::SEND_LOG_NOT_FAILED));
    }

    // 3. Find channel + check enabled
    let channel = SmsChannelRepo::find_by_id(&state.pg, log.channel_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_CHANNEL_NOT_FOUND)?;
    if channel.status != "0" {
        return Err(AppError::business_with_msg(
            ResponseCode::SMS_CHANNEL_NOT_FOUND,
            "SMS channel is disabled",
        ));
    }

    // 4. Reset status to 0 (PENDING)
    SmsLogRepo::update_status(&state.pg, log_id, 0, None, None)
        .await
        .into_internal()?;

    // 5. Rebuild params from log snapshot and create client
    let sms_client: Arc<dyn SmsClient> =
        Arc::from(client::create_client(&channel.code, &channel.api_key, &channel.api_secret)?);

    // Reconstruct template params from the log's JSON snapshot
    let template_params: HashMap<String, String> = log
        .params
        .as_deref()
        .and_then(|p| serde_json::from_str(p).ok())
        .unwrap_or_default();

    // Find template to get api_template_id (not yet snapshotted in log)
    let template = SmsTemplateRepo::find_by_id(&state.pg, log.template_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_TEMPLATE_NOT_FOUND)?;

    let sms_params = SmsSendParams {
        mobile: log.mobile,
        signature: channel.signature.clone(),
        api_template_id: template.api_template_id,
        params: template_params,
    };

    // 6. Spawn background task
    let pg = state.pg.clone();
    let sem = state.sms_semaphore.clone();
    tokio::spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        execute_sms_send(&pg, log_id, sms_client, sms_params).await;
    });

    Ok(())
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Execute SMS send with retry (3 attempts, exponential back-off).
async fn execute_sms_send(
    pg: &PgPool,
    log_id: i64,
    client: Arc<dyn SmsClient>,
    params: SmsSendParams,
) {
    const MAX_RETRIES: u32 = 3;

    let mut last_err = String::new();
    for attempt in 0..MAX_RETRIES {
        if attempt > 0 {
            let delay = std::time::Duration::from_secs(2u64.pow(attempt));
            tokio::time::sleep(delay).await;
        }

        let result = client.send(params.clone()).await;
        if result.success {
            tracing::info!(log_id, "sms sent successfully");
            if let Err(e) = SmsLogRepo::update_status(pg, log_id, 1, result.api_send_code.as_deref(), None).await {
                tracing::error!(log_id, error = %e, "failed to update sms log status to SUCCESS");
            }
            return;
        }

        last_err = result
            .error_msg
            .unwrap_or_else(|| "unknown error".to_string());
        tracing::warn!(
            log_id,
            attempt = attempt + 1,
            error = %last_err,
            "sms send attempt failed"
        );
    }

    // All retries exhausted — mark as failed
    tracing::error!(log_id, error = %last_err, "sms send failed after retries");
    if let Err(e) = SmsLogRepo::update_status(pg, log_id, 2, None, Some(&last_err)).await {
        tracing::error!(log_id, error = %e, "failed to update sms log status to FAILED");
    }
}
