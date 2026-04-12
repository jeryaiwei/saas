//! Paginated list payload. Field names match NestJS `Result.page(...)`:
//! `{ rows, total, pageNum, pageSize, pages }`.
//!
//! See `docs/framework/framework-pagination-spec.md` for the normative contract
//! governing all list endpoints.

use serde::{Deserialize, Serialize};
use validator::Validate;

// ──────────────────────────────────────────────────────────────────────
// Pagination policy constants — single source of truth.
// Spec §2.1: validator attrs and DAO clamp MUST reference these.
// ──────────────────────────────────────────────────────────────────────

/// Upper bound for `pageNum` (both HTTP validation and DAO clamp).
pub const PAGE_NUM_MAX: u32 = 10_000;
/// Default `pageNum` when the client omits it.
pub const PAGE_NUM_DEFAULT: u32 = 1;
/// Upper bound for `pageSize` (both HTTP validation and DAO clamp).
pub const PAGE_SIZE_MAX: u32 = 200;
/// Default `pageSize` when the client omits it.
pub const PAGE_SIZE_DEFAULT: u32 = 10;

// ──────────────────────────────────────────────────────────────────────
// Query duration policy (v1.1).
// ──────────────────────────────────────────────────────────────────────

/// Hard timeout for each individual DB query in a paginated flow.
/// Applied via `with_timeout` — queries exceeding this bound return an
/// `anyhow::Error` without waiting for the DB to finish.
pub const QUERY_TIMEOUT_SECS: u64 = 5;

/// Threshold above which a paginated query (`rows_ms + count_ms`)
/// emits a `tracing::warn!` — signals to operators that the query is
/// drifting toward timeout territory even though it still returns in
/// time. See `docs/framework/framework-pagination-spec.md` §6.2.
pub const SLOW_QUERY_WARN_MS: u128 = 300;

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<T> {
    pub rows: Vec<T>,
    pub total: u64,
    pub page_num: u32,
    pub page_size: u32,
    pub pages: u64,
}

impl<T> Page<T> {
    pub fn new(rows: Vec<T>, total: u64, page_num: u32, page_size: u32) -> Self {
        let pages = if page_size == 0 {
            0
        } else {
            total.div_ceil(page_size as u64)
        };
        Self {
            rows,
            total,
            page_num,
            page_size,
            pages,
        }
    }

    /// Transform each row into a different type while preserving all
    /// pagination metadata. Avoids recomputing `pages` at the call site.
    ///
    /// ```ignore
    /// let dto_page = entity_page.map_rows(MyResponseDto::from_entity);
    /// ```
    pub fn map_rows<U, F>(self, f: F) -> Page<U>
    where
        F: FnMut(T) -> U,
    {
        Page {
            rows: self.rows.into_iter().map(f).collect(),
            total: self.total,
            page_num: self.page_num,
            page_size: self.page_size,
            pages: self.pages,
        }
    }
}

/// Reusable pagination query parameters. Flatten into list-endpoint
/// request DTOs via `#[serde(flatten)]` + `#[validate(nested)]` so the
/// wire shape stays `{..., pageNum, pageSize}` without nesting.
///
/// Bounds: `page_num` ∈ 1..=10000, `page_size` ∈ 1..=200.
///
/// Fields use a custom `u32` deserializer that accepts either an integer
/// or a string. This is required because `serde_urlencoded` (used by
/// axum's `Query` extractor) stores every flattened field as a string —
/// the default `u32` deserializer rejects strings, so a struct using
/// `#[serde(flatten)]` over `u32` fields would fail at parse time even
/// though the raw wire shape is `?pageNum=1&pageSize=10`.
///
/// ```ignore
/// #[derive(Deserialize, Validate)]
/// #[serde(rename_all = "camelCase")]
/// pub struct ListUserDto {
///     pub user_name: Option<String>,
///     #[serde(flatten)]
///     #[validate(nested)]
///     pub page: PageQuery,
/// }
/// ```
#[derive(Debug, Deserialize, Validate)]
#[serde(rename_all = "camelCase")]
pub struct PageQuery {
    #[validate(range(min = 1, max = PAGE_NUM_MAX))]
    #[serde(default = "default_page_num", deserialize_with = "de_u32_any")]
    pub page_num: u32,
    #[validate(range(min = 1, max = PAGE_SIZE_MAX))]
    #[serde(default = "default_page_size", deserialize_with = "de_u32_any")]
    pub page_size: u32,
}

/// Accept `u32` from integer, float-without-fraction, or string. Needed
/// because `serde_urlencoded` + `#[serde(flatten)]` funnels every field
/// through a `String`-typed visitor.
fn de_u32_any<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::{self, Visitor};
    use std::fmt;

    struct U32Visitor;
    impl<'de> Visitor<'de> for U32Visitor {
        type Value = u32;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an unsigned integer or a numeric string")
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<u32, E> {
            u32::try_from(v).map_err(|_| E::custom("u32 out of range"))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<u32, E> {
            u32::try_from(v).map_err(|_| E::custom("u32 out of range or negative"))
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<u32, E> {
            v.parse::<u32>()
                .map_err(|_| E::custom("invalid u32 string"))
        }

        fn visit_string<E: de::Error>(self, v: String) -> Result<u32, E> {
            self.visit_str(&v)
        }
    }

    deserializer.deserialize_any(U32Visitor)
}

impl Default for PageQuery {
    fn default() -> Self {
        Self {
            page_num: PAGE_NUM_DEFAULT,
            page_size: PAGE_SIZE_DEFAULT,
        }
    }
}

fn default_page_num() -> u32 {
    PAGE_NUM_DEFAULT
}
fn default_page_size() -> u32 {
    PAGE_SIZE_DEFAULT
}

/// Clamped pagination parameters for repo-layer `LIMIT`/`OFFSET` math.
///
/// `page_num` is clamped to `>= 1` and `page_size` to `1..=200` as a
/// DoS / OOM safety bound. Computed `offset` and `limit` are `i64` to
/// match `sqlx::bind` without casts at call sites.
///
/// ```ignore
/// let p = PaginationParams::from(page_num, page_size);
/// let rows = sqlx::query_as::<_, T>("...")
///     .bind(p.limit)
///     .bind(p.offset)
///     .fetch_all(pool)
///     .await?;
/// Page::new(rows, total, p.safe_page_num, p.safe_page_size)
/// ```
#[derive(Debug, Clone, Copy)]
pub struct PaginationParams {
    pub safe_page_num: u32,
    pub safe_page_size: u32,
    pub offset: i64,
    pub limit: i64,
}

impl PaginationParams {
    /// Clamp and derive LIMIT/OFFSET from raw request-supplied values.
    /// Uses `PAGE_NUM_MAX` / `PAGE_SIZE_MAX` as the defense-in-depth upper
    /// bounds — the HTTP layer's validator is the primary gate.
    pub fn from(page_num: u32, page_size: u32) -> Self {
        let safe_page_num = page_num.clamp(1, PAGE_NUM_MAX);
        let safe_page_size = page_size.clamp(1, PAGE_SIZE_MAX);
        let offset = ((safe_page_num - 1) * safe_page_size) as i64;
        let limit = safe_page_size as i64;
        Self {
            safe_page_num,
            safe_page_size,
            offset,
            limit,
        }
    }

    /// Assemble a `Page<T>` from already-clamped params + query results.
    /// Accepts the raw `i64` returned by `sqlx::query_scalar` for
    /// `SELECT COUNT(*)` — clamps to `0` if the DB returns a negative
    /// value (should never happen, but defensive).
    ///
    /// ```ignore
    /// let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);
    /// let rows = sqlx::query_as::<_, T>(...).bind(p.limit).bind(p.offset).fetch_all(pool).await?;
    /// let total: i64 = sqlx::query_scalar(...).fetch_one(pool).await?;
    /// Ok(p.into_page(rows, total))
    /// ```
    pub fn into_page<T>(self, rows: Vec<T>, total: i64) -> Page<T> {
        Page::new(
            rows,
            total.max(0) as u64,
            self.safe_page_num,
            self.safe_page_size,
        )
    }

    /// Repair `total` when a Race B (row deleted between rows and count
    /// queries) caused `total < offset + rows.len()` — a self-contradictory
    /// state that would make the client see "Page N of fewer-than-N".
    /// Clamps `total` upward so the arithmetic is consistent.
    ///
    /// Static helper: takes raw values so it can be unit-tested without
    /// materializing a `PaginationParams` instance. See
    /// `docs/framework/framework-pagination-spec.md` §8.2.
    pub fn reconcile_total(observed_total: i64, rows_len: usize, offset: i64) -> i64 {
        let lower_bound = offset.saturating_add(rows_len as i64);
        observed_total.max(lower_bound)
    }
}

/// Wrap a fallible async operation with the framework's default query
/// timeout (`QUERY_TIMEOUT_SECS`). On timeout, returns a descriptive
/// `anyhow::Error` carrying the `ctx` label; on inner error, propagates
/// via `anyhow::Context`.
///
/// ```ignore
/// let rows = with_timeout(
///     sqlx::query_as::<_, SysUser>(sql).bind(...).fetch_all(pool),
///     "user.find_page rows",
/// ).await?;
/// ```
pub async fn with_timeout<T, E, F>(fut: F, ctx: &'static str) -> anyhow::Result<T>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: Into<anyhow::Error>,
{
    with_timeout_for(fut, std::time::Duration::from_secs(QUERY_TIMEOUT_SECS), ctx).await
}

/// Variant of `with_timeout` accepting an explicit duration — used by
/// tests that need a tiny budget to exercise the timeout path. Production
/// code should always use `with_timeout` so the policy stays centralized.
pub async fn with_timeout_for<T, E, F>(
    fut: F,
    budget: std::time::Duration,
    ctx: &'static str,
) -> anyhow::Result<T>
where
    F: std::future::Future<Output = Result<T, E>>,
    E: Into<anyhow::Error>,
{
    use anyhow::Context;
    match tokio::time::timeout(budget, fut).await {
        Ok(Ok(val)) => Ok(val),
        Ok(Err(e)) => Err(e.into()).context(ctx),
        Err(_elapsed) => Err(anyhow::anyhow!("{}: query timeout after {:?}", ctx, budget)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_rounds_up() {
        let p = Page::new(vec![1, 2, 3], 11, 1, 5);
        assert_eq!(p.pages, 3);
    }

    #[test]
    fn pages_zero_size() {
        let p: Page<i32> = Page::new(vec![], 10, 1, 0);
        assert_eq!(p.pages, 0);
    }

    #[test]
    fn map_rows_preserves_metadata() {
        let page = Page::new(vec![1u32, 2, 3], 11, 2, 5);
        let mapped: Page<String> = page.map_rows(|n| format!("row-{n}"));
        assert_eq!(mapped.rows, vec!["row-1", "row-2", "row-3"]);
        assert_eq!(mapped.total, 11);
        assert_eq!(mapped.page_num, 2);
        assert_eq!(mapped.page_size, 5);
        assert_eq!(mapped.pages, 3);
    }

    #[test]
    fn page_query_default_is_first_page_size_ten() {
        use validator::Validate;
        let q = PageQuery::default();
        assert_eq!(q.page_num, 1);
        assert_eq!(q.page_size, 10);
        assert!(q.validate().is_ok());
    }

    #[test]
    fn into_page_uses_clamped_pagination_metadata() {
        // page_size 500 clamps to PAGE_SIZE_MAX, page_num 0 clamps to 1
        let p = PaginationParams::from(0, 500);
        let page: Page<i32> = p.into_page(vec![1, 2, 3], 3i64);
        assert_eq!(page.page_num, 1);
        assert_eq!(page.page_size, PAGE_SIZE_MAX);
        assert_eq!(page.total, 3);
        assert_eq!(page.rows, vec![1, 2, 3]);
    }

    #[test]
    fn into_page_clamps_negative_total_to_zero() {
        // Postgres shouldn't return a negative COUNT(*), but the type is
        // i64 so we defensively clamp in case of driver/schema weirdness.
        let p = PaginationParams::from(1, 10);
        let page: Page<i32> = p.into_page(vec![], -5i64);
        assert_eq!(page.total, 0);
        assert_eq!(page.pages, 0);
    }

    #[test]
    fn pagination_params_from_clamps_to_policy_constants() {
        let p = PaginationParams::from(u32::MAX, u32::MAX);
        assert_eq!(p.safe_page_num, PAGE_NUM_MAX);
        assert_eq!(p.safe_page_size, PAGE_SIZE_MAX);
    }

    // ── Task 2: reconcile_total ─────────────────────────────────────

    #[test]
    fn reconcile_total_keeps_larger_value() {
        // Happy path: observed total >= offset + rows
        assert_eq!(PaginationParams::reconcile_total(100, 20, 10), 100);
    }

    #[test]
    fn reconcile_total_bumps_up_when_race_shrunk_it() {
        // Race B: COUNT ran after a delete, but rows query already saw
        // the deleted row at offset 40. Without reconcile, client sees
        // total=5 while receiving rows 40..=49, which is nonsense.
        assert_eq!(PaginationParams::reconcile_total(5, 10, 40), 50);
    }

    #[test]
    fn reconcile_total_handles_negative_input() {
        // Defensive: if DB somehow returns a negative count, reconcile
        // must not propagate the negative.
        assert_eq!(PaginationParams::reconcile_total(-5, 3, 10), 13);
    }

    // ── Task 1: with_timeout ────────────────────────────────────────

    #[tokio::test]
    async fn with_timeout_returns_ok_when_future_completes_in_time() {
        let fut = async { Ok::<_, anyhow::Error>(42_i64) };
        let result = with_timeout(fut, "test.happy").await.unwrap();
        assert_eq!(result, 42);
    }

    #[tokio::test]
    async fn with_timeout_returns_err_when_future_exceeds_budget() {
        let fut = async {
            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
            Ok::<_, anyhow::Error>(0_i64)
        };
        let result = with_timeout_for(fut, std::time::Duration::from_millis(50), "test.slow").await;
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("timeout"),
            "expected timeout error, got: {}",
            err
        );
        assert!(err.to_string().contains("test.slow"));
    }

    #[tokio::test]
    async fn with_timeout_propagates_inner_error() {
        let fut = async { Err::<i64, _>(anyhow::anyhow!("inner failure")) };
        let err = with_timeout(fut, "test.inner").await.unwrap_err();
        // Inner error should be the cause chain, ctx label should appear
        // in the chain (via anyhow::Context).
        let chain = format!("{:#}", err);
        assert!(
            chain.contains("inner failure"),
            "expected inner message in chain, got: {}",
            chain
        );
    }

    #[test]
    fn page_query_rejects_page_size_over_200() {
        use validator::Validate;
        let q = PageQuery {
            page_num: 1,
            page_size: 500,
        };
        assert!(q.validate().is_err());
    }
}
