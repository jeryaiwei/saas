//! SMS log service — business orchestration (READ-ONLY).

use super::dto::{ListSmsLogDto, SmsLogResponseDto};
use crate::domain::{SmsLogListFilter, SmsLogRepo};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i64) -> Result<SmsLogResponseDto, AppError> {
    let log = SmsLogRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;
    Ok(SmsLogResponseDto::from_entity(log))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListSmsLogDto,
) -> Result<Page<SmsLogResponseDto>, AppError> {
    let page = SmsLogRepo::find_page(
        &state.pg,
        SmsLogListFilter {
            mobile: query.mobile,
            template_code: query.template_code,
            send_status: query.send_status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(SmsLogResponseDto::from_entity))
}
