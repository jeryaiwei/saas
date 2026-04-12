//! Postgres system catalog queries for integration tests.
//!
//! Primary use: assert that expected indexes listed in
//! `docs/framework/framework-pagination-indexes.md` actually exist in the DB
//! after migrations have run. Called from integration tests — not a
//! runtime check — so the async signature consumes an existing
//! `sqlx::PgPool`.

use sqlx::PgPool;

/// Row returned by `pg_indexes` queries. Public so callers can inspect
/// detailed columns if needed (e.g. for generating diagnostic reports).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct IndexRow {
    pub schemaname: String,
    pub tablename: String,
    pub indexname: String,
}

/// Query all indexes defined on `table_name` in the current schema.
/// Useful for diagnostic test output and for the `assert_index_exists`
/// helper to walk the list without making N round-trips.
pub async fn list_indexes_on_table(
    pool: &PgPool,
    table_name: &str,
) -> anyhow::Result<Vec<IndexRow>> {
    let rows = sqlx::query_as::<_, IndexRow>(
        "SELECT schemaname, tablename, indexname \
           FROM pg_indexes \
          WHERE tablename = $1 \
          ORDER BY indexname",
    )
    .bind(table_name)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Assert that the named index exists on the named table. Returns a
/// descriptive error string on failure listing what indexes DO exist
/// on the table, so the test reporter can immediately tell what's
/// missing.
///
/// # Example
///
/// ```ignore
/// assert_index_exists(
///     &pool,
///     "sys_user_tenant",
///     "idx_sys_user_tenant_tenant_status",
/// ).await.unwrap();
/// ```
pub async fn assert_index_exists(
    pool: &PgPool,
    table_name: &str,
    index_name: &str,
) -> Result<(), String> {
    let rows = list_indexes_on_table(pool, table_name)
        .await
        .map_err(|e| format!("failed to query pg_indexes: {}", e))?;

    if rows.iter().any(|r| r.indexname == index_name) {
        Ok(())
    } else {
        let existing: Vec<&str> = rows.iter().map(|r| r.indexname.as_str()).collect();
        Err(format!(
            "missing index `{}` on table `{}`; existing indexes: {:?}",
            index_name, table_name, existing
        ))
    }
}
