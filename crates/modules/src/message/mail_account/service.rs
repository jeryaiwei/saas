//! Mail account service — business orchestration.

use super::dto::{
    CreateMailAccountDto, ListMailAccountDto, MailAccountOptionDto, MailAccountResponseDto,
    UpdateMailAccountDto,
};
use crate::domain::{
    MailAccountInsertParams, MailAccountListFilter, MailAccountRepo, MailAccountUpdateParams,
};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i32) -> Result<MailAccountResponseDto, AppError> {
    let account = MailAccountRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_ACCOUNT_NOT_FOUND)?;
    Ok(MailAccountResponseDto::from_entity(account))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListMailAccountDto,
) -> Result<Page<MailAccountResponseDto>, AppError> {
    let page = MailAccountRepo::find_page(
        &state.pg,
        MailAccountListFilter {
            mail: query.mail,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(MailAccountResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn enabled_list(state: &AppState) -> Result<Vec<MailAccountOptionDto>, AppError> {
    let rows = MailAccountRepo::find_enabled_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(MailAccountOptionDto::from_entity)
        .collect())
}

#[tracing::instrument(skip_all, fields(mail = %dto.mail))]
pub async fn create(
    state: &AppState,
    dto: CreateMailAccountDto,
) -> Result<MailAccountResponseDto, AppError> {
    // Unique check
    if MailAccountRepo::exists_by_mail(&state.pg, &dto.mail, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::MAIL_ACCOUNT_EXISTS));
    }

    let account = MailAccountRepo::insert(
        &state.pg,
        MailAccountInsertParams {
            mail: dto.mail,
            username: dto.username,
            password: dto.password,
            host: dto.host,
            port: dto.port,
            ssl_enable: dto.ssl_enable,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(MailAccountResponseDto::from_entity(account))
}

#[tracing::instrument(skip_all, fields(id = %dto.id))]
pub async fn update(state: &AppState, dto: UpdateMailAccountDto) -> Result<(), AppError> {
    // Verify existence
    MailAccountRepo::find_by_id(&state.pg, dto.id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_ACCOUNT_NOT_FOUND)?;

    // Unique check when changing mail
    if let Some(ref mail) = dto.mail {
        if MailAccountRepo::exists_by_mail(&state.pg, mail, Some(dto.id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::MAIL_ACCOUNT_EXISTS));
        }
    }

    MailAccountRepo::update_by_id(
        &state.pg,
        MailAccountUpdateParams {
            id: dto.id,
            mail: dto.mail,
            username: dto.username,
            password: dto.password,
            host: dto.host,
            port: dto.port,
            ssl_enable: dto.ssl_enable,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn remove(state: &AppState, id: i32) -> Result<(), AppError> {
    MailAccountRepo::soft_delete(&state.pg, id)
        .await
        .into_internal()?;
    Ok(())
}
