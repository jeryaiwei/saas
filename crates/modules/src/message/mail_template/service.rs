//! Mail template service — business orchestration.

use super::dto::{
    CreateMailTemplateDto, ListMailTemplateDto, MailTemplateResponseDto, UpdateMailTemplateDto,
};
use crate::domain::{
    MailTemplateInsertParams, MailTemplateListFilter, MailTemplateRepo, MailTemplateUpdateParams,
};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i32) -> Result<MailTemplateResponseDto, AppError> {
    let template = MailTemplateRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_TEMPLATE_NOT_FOUND)?;
    Ok(MailTemplateResponseDto::from_entity(template))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListMailTemplateDto,
) -> Result<Page<MailTemplateResponseDto>, AppError> {
    let page = MailTemplateRepo::find_page(
        &state.pg,
        MailTemplateListFilter {
            name: query.name,
            code: query.code,
            account_id: query.account_id,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(MailTemplateResponseDto::from_entity))
}

#[tracing::instrument(skip_all, fields(code = %dto.code))]
pub async fn create(
    state: &AppState,
    dto: CreateMailTemplateDto,
) -> Result<MailTemplateResponseDto, AppError> {
    // Unique check
    if MailTemplateRepo::exists_by_code(&state.pg, &dto.code, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(ResponseCode::MAIL_TEMPLATE_CODE_EXISTS));
    }

    let template = MailTemplateRepo::insert(
        &state.pg,
        MailTemplateInsertParams {
            name: dto.name,
            code: dto.code,
            account_id: dto.account_id,
            nickname: dto.nickname,
            title: dto.title,
            content: dto.content,
            params: dto.params,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(MailTemplateResponseDto::from_entity(template))
}

#[tracing::instrument(skip_all, fields(id = %dto.id))]
pub async fn update(state: &AppState, dto: UpdateMailTemplateDto) -> Result<(), AppError> {
    // Verify existence
    MailTemplateRepo::find_by_id(&state.pg, dto.id)
        .await
        .into_internal()?
        .or_business(ResponseCode::MAIL_TEMPLATE_NOT_FOUND)?;

    // Unique check when changing code
    if let Some(ref code) = dto.code {
        if MailTemplateRepo::exists_by_code(&state.pg, code, Some(dto.id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(ResponseCode::MAIL_TEMPLATE_CODE_EXISTS));
        }
    }

    MailTemplateRepo::update_by_id(
        &state.pg,
        MailTemplateUpdateParams {
            id: dto.id,
            name: dto.name,
            code: dto.code,
            account_id: dto.account_id,
            nickname: dto.nickname,
            title: dto.title,
            content: dto.content,
            params: dto.params,
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
    MailTemplateRepo::soft_delete(&state.pg, id)
        .await
        .into_internal()?;
    Ok(())
}
