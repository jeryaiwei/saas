//! SMS template service — business orchestration.

use super::dto::{
    CreateSmsTemplateDto, ListSmsTemplateDto, SmsTemplateResponseDto, UpdateSmsTemplateDto,
};
use crate::domain::{
    SmsTemplateInsertParams, SmsTemplateListFilter, SmsTemplateRepo, SmsTemplateUpdateParams,
};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i32) -> Result<SmsTemplateResponseDto, AppError> {
    let template = SmsTemplateRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_TEMPLATE_NOT_FOUND)?;
    Ok(SmsTemplateResponseDto::from_entity(template))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListSmsTemplateDto,
) -> Result<Page<SmsTemplateResponseDto>, AppError> {
    let page = SmsTemplateRepo::find_page(
        &state.pg,
        SmsTemplateListFilter {
            name: query.name,
            code: query.code,
            channel_id: query.channel_id,
            r_type: query.r#type,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(SmsTemplateResponseDto::from_entity))
}

#[tracing::instrument(skip_all, fields(code = %dto.code))]
pub async fn create(
    state: &AppState,
    dto: CreateSmsTemplateDto,
) -> Result<SmsTemplateResponseDto, AppError> {
    // Unique check
    if SmsTemplateRepo::exists_by_code(&state.pg, &dto.code, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::SMS_TEMPLATE_CODE_EXISTS));
    }

    let template = SmsTemplateRepo::insert(
        &state.pg,
        SmsTemplateInsertParams {
            channel_id: dto.channel_id,
            code: dto.code,
            name: dto.name,
            content: dto.content,
            params: dto.params,
            api_template_id: dto.api_template_id,
            r_type: dto.r#type,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(SmsTemplateResponseDto::from_entity(template))
}

#[tracing::instrument(skip_all, fields(id = %dto.id))]
pub async fn update(state: &AppState, dto: UpdateSmsTemplateDto) -> Result<(), AppError> {
    // Verify existence
    SmsTemplateRepo::find_by_id(&state.pg, dto.id)
        .await
        .into_internal()?
        .or_business(ResponseCode::SMS_TEMPLATE_NOT_FOUND)?;

    // Unique check when changing code
    if let Some(ref code) = dto.code {
        if SmsTemplateRepo::exists_by_code(&state.pg, code, Some(dto.id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::SMS_TEMPLATE_CODE_EXISTS));
        }
    }

    SmsTemplateRepo::update_by_id(
        &state.pg,
        SmsTemplateUpdateParams {
            id: dto.id,
            channel_id: dto.channel_id,
            code: dto.code,
            name: dto.name,
            content: dto.content,
            params: dto.params,
            api_template_id: dto.api_template_id,
            r_type: dto.r#type,
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
        .context("sms_template.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        SmsTemplateRepo::soft_delete(&mut *tx, *id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("sms_template.remove: commit")
        .into_internal()?;
    Ok(())
}
