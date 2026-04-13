//! Online user service — reads Redis sessions.

use super::dto::OnlineUserResponseDto;
use crate::state::AppState;
use anyhow::Context;
use framework::auth::{session, UserSession};
use framework::error::{AppError, IntoAppError};

/// List online users by scanning Redis session keys.
#[tracing::instrument(skip_all)]
pub async fn list(state: &AppState) -> Result<Vec<OnlineUserResponseDto>, AppError> {
    let pattern = format!("{}*", state.config.redis_keys.login_session);
    let prefix_len = state.config.redis_keys.login_session.len();

    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))
        .into_internal()?;

    // SCAN for all session keys (bounded by Redis TTL expiry)
    let mut cursor: u64 = 0;
    let mut keys: Vec<String> = Vec::new();

    loop {
        let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(200)
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis SCAN: {e}"))
            .into_internal()?;

        keys.extend(batch);
        cursor = next_cursor;
        if cursor == 0 || keys.len() >= 1000 {
            break;
        }
    }

    // Fetch each session
    let mut result: Vec<OnlineUserResponseDto> = Vec::with_capacity(keys.len());
    for key in &keys {
        let raw: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis GET session: {e}"))
            .into_internal()?;

        if let Some(json) = raw {
            if let Ok(sess) = serde_json::from_str::<UserSession>(&json) {
                let token_id = key[prefix_len..].to_string();
                result.push(OnlineUserResponseDto {
                    token_id,
                    user_id: sess.user_id,
                    user_name: sess.user_name,
                    user_type: sess.user_type,
                    tenant_id: sess.tenant_id,
                    platform_id: sess.platform_id,
                    is_admin: sess.is_admin,
                });
            }
        }
    }

    Ok(result)
}

/// Force a user offline by deleting their session and blacklisting the token.
#[tracing::instrument(skip_all, fields(token_id = %token_id))]
pub async fn force_logout(state: &AppState, token_id: &str) -> Result<(), AppError> {
    session::delete(&state.redis, &state.config.redis_keys, token_id)
        .await
        .context("force_logout: delete session")
        .into_internal()?;
    session::blacklist(
        &state.redis,
        &state.config.redis_keys,
        token_id,
        state.config.redis_ttl.token_blacklist,
    )
    .await
    .context("force_logout: blacklist token")
    .into_internal()?;
    Ok(())
}
