//! SMS channel service — business orchestration.

use super::dto::{
    CreateSmsChannelDto, ListSmsChannelDto, SmsChannelOptionDto, SmsChannelResponseDto,
    UpdateSmsChannelDto,
};
use crate::domain::{
    SmsChannelInsertParams, SmsChannelListFilter, SmsChannelRepo, SmsChannelUpdateParams,
};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i32) -> Result<SmsChannelResponseDto, AppError> {
    let channel = SmsChannelRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_CHANNEL_NOT_FOUND)?;
    Ok(SmsChannelResponseDto::from_entity(channel))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListSmsChannelDto,
) -> Result<Page<SmsChannelResponseDto>, AppError> {
    let page = SmsChannelRepo::find_page(
        &state.pg,
        SmsChannelListFilter {
            name: query.name,
            code: query.code,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(SmsChannelResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn enabled_list(state: &AppState) -> Result<Vec<SmsChannelOptionDto>, AppError> {
    let rows = SmsChannelRepo::find_enabled_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(SmsChannelOptionDto::from_entity)
        .collect())
}

#[tracing::instrument(skip_all, fields(code = %dto.code))]
pub async fn create(
    state: &AppState,
    dto: CreateSmsChannelDto,
) -> Result<SmsChannelResponseDto, AppError> {
    // Unique check
    if SmsChannelRepo::exists_by_code(&state.pg, &dto.code, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::SMS_CHANNEL_CODE_EXISTS));
    }

    let channel = SmsChannelRepo::insert(
        &state.pg,
        SmsChannelInsertParams {
            code: dto.code,
            name: dto.name,
            signature: dto.signature,
            api_key: dto.api_key,
            api_secret: dto.api_secret,
            callback_url: dto.callback_url,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(SmsChannelResponseDto::from_entity(channel))
}

#[tracing::instrument(skip_all, fields(id = %dto.id))]
pub async fn update(state: &AppState, dto: UpdateSmsChannelDto) -> Result<(), AppError> {
    // Verify existence
    SmsChannelRepo::find_by_id(&state.pg, dto.id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_CHANNEL_NOT_FOUND)?;

    // Unique check when changing code
    if let Some(ref code) = dto.code {
        if SmsChannelRepo::exists_by_code(&state.pg, code, Some(dto.id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::SMS_CHANNEL_CODE_EXISTS));
        }
    }

    SmsChannelRepo::update_by_id(
        &state.pg,
        SmsChannelUpdateParams {
            id: dto.id,
            code: dto.code,
            name: dto.name,
            signature: dto.signature,
            api_key: dto.api_key,
            api_secret: dto.api_secret,
            callback_url: dto.callback_url,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(())
}

/// Batch soft-delete. IDs are comma-separated in the URL path.
#[tracing::instrument(skip_all, fields(ids = %ids_str))]
pub async fn remove(state: &AppState, ids_str: &str) -> Result<(), AppError> {
    let ids: Vec<i32> = ids_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i32>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::business(ResponseCode::PARAM_INVALID))?;
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }
    let mut tx = state
        .pg
        .begin()
        .await
        .context("sms_channel.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        SmsChannelRepo::soft_delete(&mut *tx, *id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("sms_channel.remove: commit")
        .into_internal()?;
    Ok(())
}
