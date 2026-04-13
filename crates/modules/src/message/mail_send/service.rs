//! Mail send service — business orchestration for sending emails.

use super::dto::*;
use crate::domain::{
    MailAccountRepo, MailLogInsertParams, MailLogRepo, MailTemplateRepo, SysMailAccount,
};
use crate::message::template_parser;
use crate::state::AppState;
use framework::context::RequestContext;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::infra::smtp::{self, MailMessage, SmtpParams};
use framework::response::ResponseCode;
use sqlx::PgPool;
use std::collections::HashMap;

// ─── Public API ──────────────────────────────────────────────────────────────

#[tracing::instrument(skip_all, fields(to_mail = %dto.to_mail, template_code = %dto.template_code))]
pub async fn send(state: &AppState, dto: SendMailDto) -> Result<SendMailResponseDto, AppError> {
    // 1. Find enabled template by code
    let template = MailTemplateRepo::find_enabled_by_code(&state.pg, &dto.template_code)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_TEMPLATE_NOT_FOUND)?;

    // 2. Find account and check enabled
    let account = MailAccountRepo::find_by_id(&state.pg, template.account_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_ACCOUNT_NOT_FOUND)?;
    check_account_enabled(&account)?;

    // 3. Parse template params
    let params = dto.params.unwrap_or_default();
    let (rendered_title, rendered_content) = render_template(&template.title, &template.content, &params)?;

    // 4. Get current user info if available
    let (user_id, user_type) = current_user_info();

    // 5. Write mail log
    let log = MailLogRepo::insert(
        &state.pg,
        MailLogInsertParams {
            user_id,
            user_type,
            to_mail: dto.to_mail.clone(),
            account_id: account.id,
            from_mail: account.mail.clone(),
            template_id: template.id,
            template_code: template.code.clone(),
            template_nickname: template.nickname.clone(),
            template_title: rendered_title.clone(),
            template_content: rendered_content.clone(),
            template_params: serde_json::to_string(&params).ok(),
        },
    )
    .await
    .into_internal()?;

    // 6. Spawn background send task
    let smtp_params = build_smtp_params(&account);
    let mail_msg = MailMessage {
        from_name: template.nickname.clone(),
        from_mail: account.mail.clone(),
        to_mail: dto.to_mail,
        subject: rendered_title,
        html_body: rendered_content,
    };
    spawn_send_task(state, log.id, smtp_params, mail_msg);

    Ok(SendMailResponseDto { log_id: log.id })
}

#[tracing::instrument(skip_all, fields(template_code = %dto.template_code, count = dto.to_mails.len()))]
pub async fn batch_send(
    state: &AppState,
    dto: BatchSendMailDto,
) -> Result<BatchSendMailResponseDto, AppError> {
    // 1. Validate batch size
    if dto.to_mails.len() > 100 {
        return Err(AppError::business(ResponseCode::BATCH_SIZE_EXCEEDED));
    }

    // 2. Find template + account
    let template = MailTemplateRepo::find_enabled_by_code(&state.pg, &dto.template_code)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_TEMPLATE_NOT_FOUND)?;

    let account = MailAccountRepo::find_by_id(&state.pg, template.account_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_ACCOUNT_NOT_FOUND)?;
    check_account_enabled(&account)?;

    // 3. Parse template params
    let params = dto.params.unwrap_or_default();
    let (rendered_title, rendered_content) = render_template(&template.title, &template.content, &params)?;

    let (user_id, user_type) = current_user_info();
    let count = dto.to_mails.len() as i32;

    // 4. For each recipient: insert log + spawn task
    for to_mail in dto.to_mails {
        let log = MailLogRepo::insert(
            &state.pg,
            MailLogInsertParams {
                user_id: user_id.clone(),
                user_type,
                to_mail: to_mail.clone(),
                account_id: account.id,
                from_mail: account.mail.clone(),
                template_id: template.id,
                template_code: template.code.clone(),
                template_nickname: template.nickname.clone(),
                template_title: rendered_title.clone(),
                template_content: rendered_content.clone(),
                template_params: serde_json::to_string(&params).ok(),
            },
        )
        .await
        .into_internal()?;

        let smtp_params = build_smtp_params(&account);
        let mail_msg = MailMessage {
            from_name: template.nickname.clone(),
            from_mail: account.mail.clone(),
            to_mail,
            subject: rendered_title.clone(),
            html_body: rendered_content.clone(),
        };
        spawn_send_task(state, log.id, smtp_params, mail_msg);
    }

    Ok(BatchSendMailResponseDto { count })
}

#[tracing::instrument(skip_all, fields(log_id = %log_id))]
pub async fn resend(state: &AppState, log_id: i64) -> Result<(), AppError> {
    // 1. Find log
    let log = MailLogRepo::find_by_id(&state.pg, log_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SEND_LOG_NOT_FOUND)?;

    // 2. Check status is FAILED (2)
    if log.send_status != 2 {
        return Err(AppError::business(ResponseCode::SEND_LOG_NOT_FAILED));
    }

    // 3. Find account and check enabled
    let account = MailAccountRepo::find_by_id(&state.pg, log.account_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_ACCOUNT_NOT_FOUND)?;
    check_account_enabled(&account)?;

    // 4. Update status to SENDING (0)
    MailLogRepo::update_status(&state.pg, log_id, 0, None)
        .await
        .into_internal()?;

    // 5. Spawn background task using log snapshot fields
    let smtp_params = build_smtp_params(&account);
    let mail_msg = MailMessage {
        from_name: log.template_nickname,
        from_mail: log.from_mail,
        to_mail: log.to_mail,
        subject: log.template_title,
        html_body: log.template_content,
    };
    spawn_send_task(state, log_id, smtp_params, mail_msg);

    Ok(())
}

#[tracing::instrument(skip_all, fields(to_mail = %dto.to_mail, account_id = %dto.account_id))]
pub async fn test_send(state: &AppState, dto: TestMailDto) -> Result<(), AppError> {
    // 1. Find account
    let account = MailAccountRepo::find_by_id(&state.pg, dto.account_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_ACCOUNT_NOT_FOUND)?;

    // 2. Build SMTP params and message
    let smtp_params = build_smtp_params(&account);
    let mail_msg = MailMessage {
        from_name: account.username.clone(),
        from_mail: account.mail.clone(),
        to_mail: dto.to_mail,
        subject: dto.title.unwrap_or_else(|| "Test Email".to_string()),
        html_body: dto.content.unwrap_or_else(|| "This is a test email.".to_string()),
    };

    // 3. Synchronously send (blocking)
    let result = tokio::task::spawn_blocking(move || smtp::send_mail(&smtp_params, &mail_msg))
        .await
        .map_err(|e| AppError::business_with_msg(ResponseCode::MAIL_SEND_FAIL, format!("spawn_blocking join: {e}")))?;

    // 4. Check result
    result.map_err(|e| AppError::business_with_msg(ResponseCode::MAIL_SEND_FAIL, e))?;

    Ok(())
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

fn check_account_enabled(account: &SysMailAccount) -> Result<(), AppError> {
    if account.status != "0" {
        return Err(AppError::business_with_msg(
            ResponseCode::MAIL_ACCOUNT_NOT_FOUND,
            "Mail account is disabled",
        ));
    }
    Ok(())
}

fn render_template(
    title: &str,
    content: &str,
    params: &HashMap<String, String>,
) -> Result<(String, String), AppError> {
    template_parser::validate_params(title, params, ResponseCode::MAIL_TEMPLATE_PARAMS_MISSING)?;
    template_parser::validate_params(content, params, ResponseCode::MAIL_TEMPLATE_PARAMS_MISSING)?;
    let rendered_title = template_parser::render(title, params);
    let rendered_content = template_parser::render(content, params);
    Ok((rendered_title, rendered_content))
}

fn current_user_info() -> (Option<String>, Option<i32>) {
    RequestContext::with_current(|c| {
        let user_id = c.user_id.clone();
        let user_type = c.user_type.as_ref().and_then(|t| t.parse::<i32>().ok());
        (user_id, user_type)
    })
    .unwrap_or((None, None))
}

fn build_smtp_params(account: &SysMailAccount) -> SmtpParams {
    SmtpParams {
        host: account.host.clone(),
        port: account.port as u16,
        ssl_enable: account.ssl_enable,
        username: account.username.clone(),
        password: account.password.clone(),
    }
}

fn spawn_send_task(state: &AppState, log_id: i64, smtp_params: SmtpParams, mail_msg: MailMessage) {
    let pg = state.pg.clone();
    let sem = state.mail_semaphore.clone();
    tokio::spawn(async move {
        let _permit = sem.acquire().await.expect("semaphore closed");
        execute_mail_send(&pg, log_id, smtp_params, mail_msg).await;
    });
}

async fn execute_mail_send(
    pg: &PgPool,
    log_id: i64,
    smtp_params: SmtpParams,
    mail_msg: MailMessage,
) {
    let result = execute_with_retry(|| async {
        let sp = smtp_params.clone();
        let mm = mail_msg.clone();
        tokio::task::spawn_blocking(move || smtp::send_mail(&sp, &mm))
            .await
            .map_err(|e| format!("spawn_blocking join: {e}"))?
    })
    .await;
    match result {
        Ok(()) => {
            MailLogRepo::update_status(pg, log_id, 1, None).await.ok();
        }
        Err(e) => {
            MailLogRepo::update_status(pg, log_id, 2, Some(&e))
                .await
                .ok();
        }
    }
}

const MAX_RETRIES: u32 = 3;

async fn execute_with_retry<F, Fut>(f: F) -> Result<(), String>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    for attempt in 0..MAX_RETRIES {
        match f().await {
            Ok(()) => return Ok(()),
            Err(e) if attempt < MAX_RETRIES - 1 => {
                tokio::time::sleep(std::time::Duration::from_secs(2u64.pow(attempt + 1))).await;
                tracing::warn!(attempt, error = %e, "mail send retry");
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}
