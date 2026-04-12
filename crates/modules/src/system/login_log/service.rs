//! LoginLog service — business orchestration.

use super::dto::{ListLoginLogDto, LoginLogResponseDto};
use crate::domain::login_log_repo::{LoginLogListFilter, LoginLogRepo};
use crate::state::AppState;
use anyhow::Context;
use framework::error::{AppError, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListLoginLogDto,
) -> Result<Page<LoginLogResponseDto>, AppError> {
    let page = LoginLogRepo::find_page(
        &state.pg,
        LoginLogListFilter {
            user_name: query.user_name,
            ipaddr: query.ipaddr,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(LoginLogResponseDto::from_entity))
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
        .context("login_log.remove: begin tx")
        .into_internal()?;
    for id in &ids {
        LoginLogRepo::soft_delete(&mut *tx, id)
            .await
            .into_internal()?;
    }
    tx.commit()
        .await
        .context("login_log.remove: commit")
        .into_internal()?;
    Ok(())
}

/// Soft delete all login log entries for the current tenant.
#[tracing::instrument(skip_all)]
pub async fn clean(state: &AppState) -> Result<(), AppError> {
    let mut tx = state
        .pg
        .begin()
        .await
        .context("login_log.clean: begin tx")
        .into_internal()?;
    LoginLogRepo::soft_delete_all(&mut *tx)
        .await
        .into_internal()?;
    tx.commit()
        .await
        .context("login_log.clean: commit")
        .into_internal()?;
    Ok(())
}
