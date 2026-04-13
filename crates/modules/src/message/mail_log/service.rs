//! Mail log service — business orchestration (READ-ONLY).

use super::dto::{ListMailLogDto, MailLogResponseDto};
use crate::domain::{MailLogListFilter, MailLogRepo};
use crate::state::AppState;
use framework::error::{AppError, BusinessCheckOption, IntoAppError};
use framework::response::{Page, ResponseCode};

#[tracing::instrument(skip_all, fields(id = %id))]
pub async fn find_by_id(state: &AppState, id: i64) -> Result<MailLogResponseDto, AppError> {
    let log = MailLogRepo::find_by_id(&state.pg, id)
        .await
        .into_internal()?
        .or_business(ResponseCode::DATA_NOT_FOUND)?;
    Ok(MailLogResponseDto::from_entity(log))
}

#[tracing::instrument(skip_all)]
pub async fn list(
    state: &AppState,
    query: ListMailLogDto,
) -> Result<Page<MailLogResponseDto>, AppError> {
    let page = MailLogRepo::find_page(
        &state.pg,
        MailLogListFilter {
            to_mail: query.to_mail,
            template_code: query.template_code,
            send_status: query.send_status,
            page: query.page,
        },
    )
    .await
    .into_internal()?;
    Ok(page.map_rows(MailLogResponseDto::from_entity))
}
