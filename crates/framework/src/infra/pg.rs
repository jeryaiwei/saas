//! PostgreSQL connection pool (sqlx).

use crate::config::PostgresConfig;
use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;

fn options(cfg: &PostgresConfig) -> PgPoolOptions {
    PgPoolOptions::new()
        .max_connections(cfg.max_connections)
        .acquire_timeout(Duration::from_secs(cfg.acquire_timeout_sec))
        .idle_timeout(Some(Duration::from_secs(cfg.idle_timeout_sec)))
}

/// Eager connect — fails at startup if the DB is unreachable.
pub async fn connect(cfg: &PostgresConfig) -> anyhow::Result<PgPool> {
    options(cfg)
        .connect(&cfg.url)
        .await
        .map_err(|e| anyhow::anyhow!("connect postgres: {e}"))
}

/// Lazy connect — defers the first TCP handshake until the pool is used.
/// Preferred for dev and cross-process smoke tests where the DB might not
/// be up yet when the server starts.
pub fn connect_lazy(cfg: &PostgresConfig) -> anyhow::Result<PgPool> {
    options(cfg)
        .connect_lazy(&cfg.url)
        .map_err(|e| anyhow::anyhow!("connect_lazy postgres: {e}"))
}

/// Readiness probe — used by `/health/ready`.
pub async fn ping(pool: &PgPool) -> anyhow::Result<()> {
    sqlx::query("SELECT 1")
        .execute(pool)
        .await
        .map_err(|e| anyhow::anyhow!("pg ping failed: {e}"))?;
    Ok(())
}
