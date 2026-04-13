//! Audit log service — read-only business logic.

use super::dto::{AuditLogResponseDto, AuditLogStatsDto, ListAuditLogDto};
use crate::domain::{AuditLogListFilter, AuditLogRepo};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};
use std::collections::HashMap;

/// Paginated audit log list.
#[tracing::instrument(skip_all, fields(
    has_action = query.action.is_some(),
    page_num = query.page.page_num,
    page_size = query.page.page_size,
))]
pub async fn list(
    state: &AppState,
    query: ListAuditLogDto,
) -> Result<Page<AuditLogResponseDto>, AppError> {
    let page = AuditLogRepo::find_page(
        &state.pg,
        AuditLogListFilter {
            action: query.action,
            module: query.module,
            user_name: query.user_name,
            status: query.status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;

    Ok(page.map_rows(AuditLogResponseDto::from_entity))
}

/// Find a single audit log by id.
#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: &str) -> Result<AuditLogResponseDto, AppError> {
    let log = AuditLogRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::AUDIT_LOG_NOT_FOUND)?;

    Ok(AuditLogResponseDto::from_entity(log))
}

/// Stats summary.
#[tracing::instrument(skip_all)]
pub async fn stats_summary(state: &AppState) -> Result<AuditLogStatsDto, AppError> {
    let (total, today_count, action_rows) = AuditLogRepo::stats_summary_full(&state.pg)
        .await
        .into_internal()?;

    let action_counts: HashMap<String, i64> = action_rows.into_iter().collect();

    Ok(AuditLogStatsDto {
        total,
        today_count,
        action_counts,
    })
}
