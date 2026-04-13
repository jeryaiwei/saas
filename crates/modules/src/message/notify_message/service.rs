//! Notify message service — business orchestration.

use super::dto::{
    ListNotifyMessageDto, MyNotifyMessageDto, NotifyMessageResponseDto, SendAllNotifyMessageDto,
    SendNotifyMessageDto, UnreadCountDto,
};
use crate::domain::{
    NotifyMessageInsertParams, NotifyMessageListFilter, NotifyMessageRepo, NotifyMyMessageFilter,
};
use crate::state::AppState;
use anyhow::Context;
use framework::context::RequestContext;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

/// Get current user id from request context.
fn current_user_id() -> Result<String, AppError> {
    RequestContext::with_current(|c| c.user_id.clone())
        .flatten()
        .ok_or_else(|| AppError::business(ResponseCode::UNAUTHORIZED))
}

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i64) -> Result<NotifyMessageResponseDto, AppError> {
    let msg = NotifyMessageRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::NOTIFY_MESSAGE_NOT_FOUND)?;
    Ok(NotifyMessageResponseDto::from_entity(msg))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListNotifyMessageDto,
) -> Result<Page<NotifyMessageResponseDto>, AppError> {
    let page = NotifyMessageRepo::find_page(
        &state.pg,
        NotifyMessageListFilter {
            template_code: query.template_code,
            user_id: query.user_id,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(NotifyMessageResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn my_list(
    state: &AppState,
    query: MyNotifyMessageDto,
) -> Result<Page<NotifyMessageResponseDto>, AppError> {
    let user_id = current_user_id()?;
    let page = NotifyMessageRepo::find_my_page(
        &state.pg,
        &user_id,
        NotifyMyMessageFilter {
            read_status: query.read_status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(NotifyMessageResponseDto::from_entity))
}

#[tracing::instrument(skip_all)]
pub async fn unread_count(state: &AppState) -> Result<UnreadCountDto, AppError> {
    let user_id = current_user_id()?;
    let count = NotifyMessageRepo::count_unread(&state.pg, &user_id)
        .await
        .into_internal()?;
    Ok(UnreadCountDto { count })
}

#[tracing::instrument(skip_all, fields(user_id = %dto.user_id))]
pub async fn send(
    state: &AppState,
    dto: SendNotifyMessageDto,
) -> Result<NotifyMessageResponseDto, AppError> {
    let msg = NotifyMessageRepo::insert(
        &state.pg,
        NotifyMessageInsertParams {
            user_id: dto.user_id,
            user_type: dto.user_type,
            template_id: dto.template_id,
            template_code: dto.template_code,
            template_nickname: dto.template_nickname,
            template_content: dto.template_content,
            template_params: dto.template_params,
        },
    )
    .await
    .into_internal()?;
    Ok(NotifyMessageResponseDto::from_entity(msg))
}

#[tracing::instrument(skip_all, fields(user_count = dto.user_ids.len()))]
pub async fn send_all(state: &AppState, dto: SendAllNotifyMessageDto) -> Result<(), AppError> {
    if dto.user_ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }
    let mut tx = state
        .pg
        .begin()
        .await
        .context("notify_message.send_all: begin tx")
        .into_internal()?;
    for user_id in &dto.user_ids {
        NotifyMessageRepo::insert(
            &mut *tx,
            NotifyMessageInsertParams {
                user_id: user_id.clone(),
                user_type: dto.user_type,
                template_id: dto.template_id,
                template_code: dto.template_code.clone(),
                template_nickname: dto.template_nickname.clone(),
                template_content: dto.template_content.clone(),
                template_params: dto.template_params.clone(),
            },
        )
        .await
        .into_internal()?;
    }
    tx.commit()
        .await
        .context("notify_message.send_all: commit")
        .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn mark_read(state: &AppState, id: i64) -> Result<(), AppError> {
    NotifyMessageRepo::mark_read(&state.pg, id)
        .await
        .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all, fields(ids = %ids_str))]
pub async fn mark_read_batch(state: &AppState, ids_str: &str) -> Result<(), AppError> {
    let ids: Vec<i64> = ids_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::business(ResponseCode::PARAM_INVALID))?;
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }
    let mut tx = state
        .pg
        .begin()
        .await
        .context("notify_message.mark_read_batch: begin tx")
        .into_internal()?;
    for id in &ids {
        NotifyMessageRepo::mark_read(&mut *tx, *id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("notify_message.mark_read_batch: commit")
        .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all)]
pub async fn mark_all_read(state: &AppState) -> Result<(), AppError> {
    let user_id = current_user_id()?;
    NotifyMessageRepo::mark_all_read(&state.pg, &user_id)
        .await
        .into_internal()?;
    Ok(())
}

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn remove(state: &AppState, id: i64) -> Result<(), AppError> {
    NotifyMessageRepo::soft_delete(&state.pg, id)
        .await
        .into_internal()?;
    Ok(())
}

/// Batch soft-delete. IDs are comma-separated in the URL path.
#[tracing::instrument(skip_all, fields(ids = %ids_str))]
pub async fn remove_batch(state: &AppState, ids_str: &str) -> Result<(), AppError> {
    let ids: Vec<i64> = ids_str
        .split(',')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.parse::<i64>())
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| AppError::business(ResponseCode::PARAM_INVALID))?;
    if ids.is_empty() {
        return Err(AppError::business(ResponseCode::PARAM_INVALID));
    }
    let mut tx = state
        .pg
        .begin()
        .await
        .context("notify_message.remove_batch: begin tx")
        .into_internal()?;
    for id in &ids {
        NotifyMessageRepo::soft_delete(&mut *tx, *id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("notify_message.remove_batch: commit")
        .into_internal()?;
    Ok(())
}
