//! Notify template service — business orchestration.

use super::dto::{
    CreateNotifyTemplateDto, ListNotifyTemplateDto, NotifyTemplateOptionDto,
    NotifyTemplateResponseDto, UpdateNotifyTemplateDto,
};
use crate::domain::{
    NotifyTemplateInsertParams, NotifyTemplateListFilter, NotifyTemplateRepo,
    NotifyTemplateUpdateParams,
};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i32) -> Result<NotifyTemplateResponseDto, AppError> {
    let template = NotifyTemplateRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::NOTIFY_TEMPLATE_NOT_FOUND)?;
    Ok(NotifyTemplateResponseDto::from_entity(template))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListNotifyTemplateDto,
) -> Result<Page<NotifyTemplateResponseDto>, AppError> {
    let page = NotifyTemplateRepo::find_page(
        &state.pg,
        NotifyTemplateListFilter {
            name: query.name,
            r_type: query.r#type,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(NotifyTemplateResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn option_select(state: &AppState) -> Result<Vec<NotifyTemplateOptionDto>, AppError> {
    let rows = NotifyTemplateRepo::find_option_list(&state.pg)
        .await
        .into_internal()?;
    Ok(rows
        .into_iter()
        .map(NotifyTemplateOptionDto::from_entity)
        .collect())
}

#[tracing::instrument(skip_all, fields(code = %dto.code))]
pub async fn create(
    state: &AppState,
    dto: CreateNotifyTemplateDto,
) -> Result<NotifyTemplateResponseDto, AppError> {
    // Unique check
    if NotifyTemplateRepo::exists_by_code(&state.pg, &dto.code, None)
        .await
        .into_internal()?
    {
        return Err(AppError::business(
            ResponseCode::NOTIFY_TEMPLATE_CODE_EXISTS,
        ));
    }

    let template = NotifyTemplateRepo::insert(
        &state.pg,
        NotifyTemplateInsertParams {
            name: dto.name,
            code: dto.code,
            nickname: dto.nickname,
            content: dto.content,
            params: dto.params,
            r_type: dto.r#type,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(NotifyTemplateResponseDto::from_entity(template))
}

#[tracing::instrument(skip_all, fields(id = %dto.id))]
pub async fn update(state: &AppState, dto: UpdateNotifyTemplateDto) -> Result<(), AppError> {
    // Verify existence
    NotifyTemplateRepo::find_by_id(&state.pg, dto.id)
        .await
        .into_internal()?
        .or_business(ResponseCode::NOTIFY_TEMPLATE_NOT_FOUND)?;

    // Unique check when changing code
    if let Some(ref code) = dto.code {
        if NotifyTemplateRepo::exists_by_code(&state.pg, code, Some(dto.id))
            .await
            .into_internal()?
        {
            return Err(AppError::business(
                ResponseCode::NOTIFY_TEMPLATE_CODE_EXISTS,
            ));
        }
    }

    NotifyTemplateRepo::update_by_id(
        &state.pg,
        NotifyTemplateUpdateParams {
            id: dto.id,
            name: dto.name,
            code: dto.code,
            nickname: dto.nickname,
            content: dto.content,
            params: dto.params,
            r_type: dto.r#type,
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
    NotifyTemplateRepo::soft_delete(&state.pg, id)
        .await
        .into_internal()?;
    Ok(())
}
