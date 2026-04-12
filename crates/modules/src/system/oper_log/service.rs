//! OperLog service — business orchestration.

use super::dto::{ListOperLogDto, OperLogResponseDto};
use crate::domain::oper_log_repo::{OperLogListFilter, OperLogRepo};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(oper_id = %oper_id))]
pub async fn find_by_id(state: &AppState, oper_id: &str) -> Result<OperLogResponseDto, AppError> {
    let log = OperLogRepo::find_by_id(&state.pg, oper_id)
        .await
        .into_internal()?
        .or_business(ResponseCode::OPER_LOG_NOT_FOUND)?;
    Ok(OperLogResponseDto::from_entity(log))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListOperLogDto,
) -> Result<Page<OperLogResponseDto>, AppError> {
    let page = OperLogRepo::find_page(
        &state.pg,
        OperLogListFilter {
            title: query.title,
            oper_name: query.oper_name,
            business_type: query.business_type,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(OperLogResponseDto::from_entity))
}

/// Hard delete a single oper log entry.
#[tracing::instrument(skip_all, fields(oper_id = %oper_id))]
pub async fn remove(state: &AppState, oper_id: &str) -> Result<(), AppError> {
    let ids: Vec<&str> = oper_id
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
        .context("oper_log.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        OperLogRepo::delete_by_id(&mut *tx, id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("oper_log.remove: commit")
        .into_internal()?;
    Ok(())
}

/// Hard delete all oper log entries for the current tenant.
#[tracing::instrument(skip_all)]
pub async fn clean(state: &AppState) -> Result<(), AppError> {
    let mut tx = state
        .pg
        .begin()
        .await
        .context("oper_log.clean: begin tx")
        .into_internal()?;
    OperLogRepo::delete_all(&mut *tx).await.into_internal()?;
    tx.commit()
        .await
        .context("oper_log.clean: commit")
        .into_internal()?;
    Ok(())
}
