//! Redis connection pool (deadpool-redis) + typed helper extensions.

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

// ─── Typed Redis helpers ────────────────────────────────────────────────────

/// Extension trait for `RedisPool` providing strongly-typed operations.
///
/// ```ignore
/// use framework::infra::redis::RedisExt;
///
/// pool.set_ex("key", &my_struct, 600).await?;
/// let val: Option<MyStruct> = pool.get_json("key").await?;
/// pool.del("key").await?;
/// ```
#[async_trait::async_trait]
pub trait RedisExt {
    /// SET key with TTL (seconds). Serializes `value` to JSON.
    async fn set_ex<T: serde::Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: u64,
    ) -> anyhow::Result<()>;

    /// GET key and deserialize from JSON. Returns `None` if key not found.
    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> anyhow::Result<Option<T>>;

    /// SET key with TTL, raw string value (no JSON serialization).
    async fn set_ex_raw(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()>;

    /// GET raw string value.
    async fn get_raw(&self, key: &str) -> anyhow::Result<Option<String>>;

    /// Check if key exists.
    async fn exists(&self, key: &str) -> anyhow::Result<bool>;

    /// INCR key by 1. Returns the new value.
    async fn incr(&self, key: &str) -> anyhow::Result<i64>;

    /// Set TTL (seconds) on an existing key.
    async fn expire(&self, key: &str, ttl_secs: u64) -> anyhow::Result<()>;

    /// INCR key and set TTL. Returns the new value.
    async fn incr_ex(&self, key: &str, ttl_secs: u64) -> anyhow::Result<i64>;

    /// DELETE key.
    async fn del(&self, key: &str) -> anyhow::Result<()>;
}

#[async_trait::async_trait]
impl RedisExt for RedisPool {
    async fn set_ex<T: serde::Serialize + Send + Sync>(
        &self,
        key: &str,
        value: &T,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let json = serde_json::to_string(value)
            .map_err(|e| anyhow::anyhow!("redis set_ex serialize: {e}"))?;
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let _: () = redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_secs)
            .arg(&json)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis SETEX: {e}"))?;
        Ok(())
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
    ) -> anyhow::Result<Option<T>> {
        let raw = self.get_raw(key).await?;
        match raw {
            None => Ok(None),
            Some(s) => {
                let val = serde_json::from_str(&s)
                    .map_err(|e| anyhow::anyhow!("redis get_json deserialize: {e}"))?;
                Ok(Some(val))
            }
        }
    }

    async fn set_ex_raw(
        &self,
        key: &str,
        value: &str,
        ttl_secs: u64,
    ) -> anyhow::Result<()> {
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let _: () = redis::cmd("SETEX")
            .arg(key)
            .arg(ttl_secs)
            .arg(value)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis SETEX: {e}"))?;
        Ok(())
    }

    async fn get_raw(&self, key: &str) -> anyhow::Result<Option<String>> {
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let val: Option<String> = redis::cmd("GET")
            .arg(key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis GET: {e}"))?;
        Ok(val)
    }

    async fn exists(&self, key: &str) -> anyhow::Result<bool> {
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let val: i64 = redis::cmd("EXISTS")
            .arg(key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis EXISTS: {e}"))?;
        Ok(val > 0)
    }

    async fn incr(&self, key: &str) -> anyhow::Result<i64> {
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let val: i64 = redis::cmd("INCR")
            .arg(key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis INCR: {e}"))?;
        Ok(val)
    }

    async fn expire(&self, key: &str, ttl_secs: u64) -> anyhow::Result<()> {
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let _: () = redis::cmd("EXPIRE")
            .arg(key)
            .arg(ttl_secs)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis EXPIRE: {e}"))?;
        Ok(())
    }

    async fn incr_ex(&self, key: &str, ttl_secs: u64) -> anyhow::Result<i64> {
        let val = self.incr(key).await?;
        self.expire(key, ttl_secs).await?;
        Ok(val)
    }

    async fn del(&self, key: &str) -> anyhow::Result<()> {
        let mut conn = self
            .get()
            .await
            .map_err(|e| anyhow::anyhow!("redis get conn: {e}"))?;
        let _: i64 = redis::cmd("DEL")
            .arg(key)
            .query_async(&mut *conn)
            .await
            .map_err(|e| anyhow::anyhow!("redis DEL: {e}"))?;
        Ok(())
    }
}
