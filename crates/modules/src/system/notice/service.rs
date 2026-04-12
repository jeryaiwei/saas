//! Notice service — business orchestration.

use super::dto::{CreateNoticeDto, ListNoticeDto, NoticeResponseDto, UpdateNoticeDto};
use crate::domain::{NoticeInsertParams, NoticeListFilter, NoticeRepo, NoticeUpdateParams};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(notice_id = %notice_id))]
pub async fn find_by_id(state: &AppState, notice_id: &str) -> Result<NoticeResponseDto, AppError> {
    let notice = NoticeRepo::find_by_id(&state.pg, notice_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::NOTICE_NOT_FOUND)?;
    Ok(NoticeResponseDto::from_entity(notice))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListNoticeDto,
) -> Result<Page<NoticeResponseDto>, AppError> {
    let page = NoticeRepo::find_page(
        &state.pg,
        NoticeListFilter {
            notice_title: query.notice_title,
            notice_type: query.notice_type,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(NoticeResponseDto::from_entity))
}

#[tracing::instrument(skip_all, fields(notice_title = %dto.notice_title))]
pub async fn create(state: &AppState, dto: CreateNoticeDto) -> Result<NoticeResponseDto, AppError> {
    let notice = NoticeRepo::insert(
        &state.pg,
        NoticeInsertParams {
            notice_title: dto.notice_title,
            notice_type: dto.notice_type,
            notice_content: dto.notice_content,
            status: dto.status,
            remark: dto.remark,
        },
    )
    .await
    .into_internal()?;

    Ok(NoticeResponseDto::from_entity(notice))
}

#[tracing::instrument(skip_all, fields(notice_id = %dto.notice_id))]
pub async fn update(state: &AppState, dto: UpdateNoticeDto) -> Result<(), AppError> {
    // Verify existence
    NoticeRepo::find_by_id(&state.pg, &dto.notice_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::NOTICE_NOT_FOUND)?;

    NoticeRepo::update_by_id(
        &state.pg,
        NoticeUpdateParams {
            notice_id: dto.notice_id,
            notice_title: dto.notice_title,
            notice_type: dto.notice_type,
            notice_content: dto.notice_content,
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
    let ids: Vec<&str> = ids_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }
    let mut tx = state
        .pg
        .begin()
        .await
        .context("notice.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        NoticeRepo::soft_delete(&mut *tx, id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("notice.remove: commit")
        .into_internal()?;
    Ok(())
}
