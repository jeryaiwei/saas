//! ConfigRepo — hand-written SQL for sys_config.
//!
//! PLATFORM tenant model — filtered by platform_id (shared across
//! all tenants within the same platform).

use super::entities::SysConfig;
use anyhow::Context;
use framework::context::{audit_update_by, current_platform_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    config_id, tenant_id, config_name, config_key, config_value, \
    config_type, create_by, create_at, update_by, update_at, \
    remark, status, del_flag";

const CONFIG_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR config_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR config_key LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR config_type = $4)";

#[derive(Debug)]
pub struct ConfigListFilter {
    pub config_name: Option<String>,
    pub config_key: Option<String>,
    pub config_type: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct ConfigInsertParams {
    pub config_name: String,
    pub config_key: String,
    pub config_value: String,
    pub config_type: String,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct ConfigUpdateParams {
    pub config_id: String,
    pub config_name: Option<String>,
    pub config_key: Option<String>,
    pub config_value: Option<String>,
    pub config_type: Option<String>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct ConfigRepo;

impl ConfigRepo {
    #[instrument(skip_all, fields(config_id = %config_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        config_id: &str,
    ) -> anyhow::Result<Option<SysConfig>> {
        let platform = current_platform_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_config \
             WHERE config_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysConfig>(&sql)
            .bind(config_id)
            .bind(platform.as_deref())
            .fetch_optional(executor)
            .await
            .context("config.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(config_key = %config_key))]
    pub async fn find_by_key(
        executor: impl sqlx::PgExecutor<'_>,
        config_key: &str,
    ) -> anyhow::Result<Option<SysConfig>> {
        let platform = current_platform_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_config \
             WHERE config_key = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysConfig>(&sql)
            .bind(config_key)
            .bind(platform.as_deref())
            .fetch_optional(executor)
            .await
            .context("config.find_by_key")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(config_key = %config_key))]
    pub async fn exists_by_key(
        executor: impl sqlx::PgExecutor<'_>,
        config_key: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let platform = current_platform_scope();
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_config \
                WHERE config_key = $1 AND del_flag = '0' \
                  AND ($2::varchar IS NULL OR tenant_id = $2) \
                  AND ($3::varchar IS NULL OR config_id <> $3)\
            )",
        )
        .bind(config_key)
        .bind(platform.as_deref())
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("config.exists_by_key")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(
        has_name = filter.config_name.is_some(),
        has_key = filter.config_key.is_some(),
        has_type = filter.config_type.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: ConfigListFilter,
    ) -> anyhow::Result<framework::response::Page<SysConfig>> {
        let mut conn = conn.acquire().await.context("config.find_page: acquire")?;
        let platform = current_platform_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_config {CONFIG_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysConfig>(&rows_sql)
                .bind(platform.as_deref())
                .bind(filter.config_name.as_deref())
                .bind(filter.config_key.as_deref())
                .bind(filter.config_type.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "config.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_config {CONFIG_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(platform.as_deref())
                .bind(filter.config_name.as_deref())
                .bind(filter.config_key.as_deref())
                .bind(filter.config_type.as_deref())
                .fetch_one(&mut *conn),
            "config.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(rows_ms, count_ms, total_ms, "config.find_page: slow query");
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    #[instrument(skip_all, fields(config_key = %params.config_key))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: ConfigInsertParams,
    ) -> anyhow::Result<SysConfig> {
        let audit = AuditInsert::now();
        let platform = current_platform_scope().context("config.insert: platform_id required")?;
        let sql = format!(
            "INSERT INTO sys_config (\
                config_id, tenant_id, config_name, config_key, config_value, \
                config_type, status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, $5, $6, '0', $7, $8, \
                CURRENT_TIMESTAMP, $9\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysConfig>(&sql)
            .bind(&platform)
            .bind(&params.config_name)
            .bind(&params.config_key)
            .bind(&params.config_value)
            .bind(&params.config_type)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("config.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(config_id = %params.config_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: ConfigUpdateParams,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_config SET \
                config_name  = COALESCE($3, config_name), \
                config_key   = COALESCE($4, config_key), \
                config_value = COALESCE($5, config_value), \
                config_type  = COALESCE($6, config_type), \
                status       = COALESCE($7, status), \
                remark       = CASE WHEN $8::boolean THEN $9 ELSE remark END, \
                update_by    = $10, \
                update_at    = CURRENT_TIMESTAMP \
             WHERE config_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(&params.config_id)
        .bind(platform.as_deref())
        .bind(params.config_name.as_deref())
        .bind(params.config_key.as_deref())
        .bind(params.config_value.as_deref())
        .bind(params.config_type.as_deref())
        .bind(params.status.as_deref())
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("config.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    /// Update config value by config_key (platform-scoped).
    #[instrument(skip_all, fields(config_key = %config_key))]
    pub async fn update_value_by_key(
        executor: impl sqlx::PgExecutor<'_>,
        config_key: &str,
        config_value: &str,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_config SET \
                config_value = $3, update_by = $4, update_at = CURRENT_TIMESTAMP \
             WHERE config_key = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(config_key)
        .bind(platform.as_deref())
        .bind(config_value)
        .bind(&update_by)
        .execute(executor)
        .await
        .context("config.update_value_by_key")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(config_id = %config_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        config_id: &str,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_config SET del_flag = '1', update_by = $3, update_at = CURRENT_TIMESTAMP \
             WHERE config_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(config_id)
        .bind(platform.as_deref())
        .bind(&update_by)
        .execute(executor)
        .await
        .context("config.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}
