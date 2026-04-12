//! Redis connection pool (deadpool-redis).

use crate::config::RedisConfig;
use deadpool_redis::{Config, Pool, PoolConfig, Runtime};

pub type RedisPool = Pool;

pub fn build(cfg: &RedisConfig) -> anyhow::Result<RedisPool> {
    let mut dp_cfg = Config::from_url(&cfg.url);
    dp_cfg.pool = Some(PoolConfig::new(cfg.pool_size as usize));
    let pool = dp_cfg
        .create_pool(Some(Runtime::Tokio1))
        .map_err(|e| anyhow::anyhow!("build redis pool: {e}"))?;
    Ok(pool)
}

/// Readiness probe — used by `/health/ready`.
pub async fn ping(pool: &RedisPool) -> anyhow::Result<()> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
    let res: String = redis::cmd("PING")
        .query_async(&mut conn)
        .await
        .map_err(|e| anyhow::anyhow!("redis ping: {e}"))?;
    if res != "PONG" {
        anyhow::bail!("redis ping unexpected response: {res}");
    }
    Ok(())
}
