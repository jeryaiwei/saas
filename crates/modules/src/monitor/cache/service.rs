//! Cache monitor service — pure Redis operations.

use super::dto::{CacheInfoDto, CacheKeyDto, CacheNameDto, CacheValueDto};
use crate::state::AppState;
use framework::error::{AppError, IntoAppError};

/// Parse a value from Redis INFO output for a given key.
fn parse_info_field(info: &str, key: &str) -> String {
    for line in info.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(key) {
            if let Some(val) = rest.strip_prefix(':') {
                return val.to_string();
            }
        }
    }
    String::new()
}

fn parse_info_i64(info: &str, key: &str) -> i64 {
    parse_info_field(info, key).parse::<i64>().unwrap_or(0)
}

/// Get Redis server info and db size.
#[tracing::instrument(skip_all)]
pub async fn get_info(state: &AppState) -> Result<CacheInfoDto, AppError> {
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis: {e}"))
        .into_internal()?;

    let info: String = redis::cmd("INFO")
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis INFO: {e}"))
        .into_internal()?;

    let db_size: i64 = redis::cmd("DBSIZE")
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis DBSIZE: {e}"))
        .into_internal()?;

    Ok(CacheInfoDto {
        redis_version: parse_info_field(&info, "redis_version"),
        used_memory: parse_info_field(&info, "used_memory"),
        used_memory_human: parse_info_field(&info, "used_memory_human"),
        connected_clients: parse_info_i64(&info, "connected_clients"),
        maxmemory_human: parse_info_field(&info, "maxmemory_human"),
        uptime_in_days: parse_info_i64(&info, "uptime_in_days"),
        db_size,
    })
}

/// Return the predefined cache name categories.
pub fn get_names() -> Vec<CacheNameDto> {
    vec![
        CacheNameDto {
            cache_name: "login_token_session:".to_string(),
            remark: "登录会话".to_string(),
        },
        CacheNameDto {
            cache_name: "sys_config:".to_string(),
            remark: "系统配置".to_string(),
        },
        CacheNameDto {
            cache_name: "sys_dict:".to_string(),
            remark: "字典缓存".to_string(),
        },
        CacheNameDto {
            cache_name: "captcha_code:".to_string(),
            remark: "验证码".to_string(),
        },
    ]
}

/// SCAN for keys matching `{cache_name}*`, return up to 100 keys.
#[tracing::instrument(skip_all, fields(cache_name = %cache_name))]
pub async fn get_keys(state: &AppState, cache_name: &str) -> Result<Vec<CacheKeyDto>, AppError> {
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis: {e}"))
        .into_internal()?;

    let pattern = format!("{cache_name}*");
    let mut cursor: u64 = 0;
    let mut keys: Vec<String> = Vec::new();

    loop {
        let (next_cursor, batch): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(&pattern)
            .arg("COUNT")
            .arg(100)
            .query_async(&mut conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis SCAN: {e}"))
            .into_internal()?;

        keys.extend(batch);
        cursor = next_cursor;
        if cursor == 0 || keys.len() >= 100 {
            break;
        }
    }

    keys.truncate(100);
    Ok(keys
        .into_iter()
        .map(|cache_key| CacheKeyDto { cache_key })
        .collect())
}

/// GET a key's value and TTL.
#[tracing::instrument(skip_all, fields(cache_key = %cache_key))]
pub async fn get_value(state: &AppState, cache_key: &str) -> Result<CacheValueDto, AppError> {
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis: {e}"))
        .into_internal()?;

    let cache_value: String = redis::cmd("GET")
        .arg(cache_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis GET: {e}"))
        .into_internal()?;

    let ttl: i64 = redis::cmd("TTL")
        .arg(cache_key)
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis TTL: {e}"))
        .into_internal()?;

    Ok(CacheValueDto {
        cache_key: cache_key.to_string(),
        cache_value,
        ttl,
    })
}

/// SCAN + DEL all keys matching `{cache_name}*`.
#[tracing::instrument(skip_all, fields(cache_name = %cache_name))]
pub async fn clear_cache_name(state: &AppState, cache_name: &str) -> Result<(), AppError> {
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis: {e}"))
        .into_internal()?;

    let pattern = format!("{cache_name}*");
    let mut cursor: u64 = 0;

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

        if !batch.is_empty() {
            let mut del = redis::cmd("DEL");
            for key in &batch {
                del.arg(key);
            }
            del.query_async::<()>(&mut conn)
                .await
                .map_err(|e| anyhow::anyhow!("redis DEL: {e}"))
                .into_internal()?;
        }

        cursor = next_cursor;
        if cursor == 0 {
            break;
        }
    }

    Ok(())
}

/// DEL a single key.
#[tracing::instrument(skip_all, fields(cache_key = %cache_key))]
pub async fn clear_cache_key(state: &AppState, cache_key: &str) -> Result<(), AppError> {
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis: {e}"))
        .into_internal()?;

    redis::cmd("DEL")
        .arg(cache_key)
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis DEL: {e}"))
        .into_internal()?;

    Ok(())
}

/// FLUSHDB — clear all keys in the current database.
#[tracing::instrument(skip_all)]
pub async fn clear_all(state: &AppState) -> Result<(), AppError> {
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis: {e}"))
        .into_internal()?;

    redis::cmd("FLUSHDB")
        .query_async::<()>(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis FLUSHDB: {e}"))
        .into_internal()?;

    Ok(())
}
