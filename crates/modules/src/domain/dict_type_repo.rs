//! DictTypeRepo — hand-written SQL for sys_dict_type.
//!
//! PLATFORM tenant model — filtered by platform_id.

use super::entities::SysDictType;
use anyhow::Context;
use framework::context::{audit_update_by, current_platform_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    dict_id, tenant_id, dict_name, dict_type, status, \
    create_by, create_at, update_by, update_at, remark, del_flag, i18n";

const DICT_TYPE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR dict_name LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR dict_type LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR status = $4)";

#[derive(Debug)]
pub struct DictTypeListFilter {
    pub dict_name: Option<String>,
    pub dict_type: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct DictTypeInsertParams {
    pub dict_name: String,
    pub dict_type: String,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct DictTypeUpdateParams {
    pub dict_id: String,
    pub dict_name: Option<String>,
    pub dict_type: Option<String>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct DictTypeRepo;

impl DictTypeRepo {
    #[instrument(skip_all, fields(dict_id = %dict_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        dict_id: &str,
    ) -> anyhow::Result<Option<SysDictType>> {
        let platform = current_platform_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_dict_type \
             WHERE dict_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysDictType>(&sql)
            .bind(dict_id)
            .bind(platform.as_deref())
            .fetch_optional(executor)
            .await
            .context("dict_type.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(dict_type = %dict_type))]
    pub async fn exists_by_type(
        executor: impl sqlx::PgExecutor<'_>,
        dict_type: &str,
        exclude_id: Option<&str>,
    ) -> anyhow::Result<bool> {
        let platform = current_platform_scope();
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_dict_type \
                WHERE dict_type = $1 AND del_flag = '0' \
                  AND ($2::varchar IS NULL OR tenant_id = $2) \
                  AND ($3::varchar IS NULL OR dict_id <> $3)\
            )",
        )
        .bind(dict_type)
        .bind(platform.as_deref())
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("dict_type.exists_by_type")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(
        has_name = filter.dict_name.is_some(),
        has_type = filter.dict_type.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: DictTypeListFilter,
    ) -> anyhow::Result<framework::response::Page<SysDictType>> {
        let mut conn = conn
            .acquire()
            .await
            .context("dict_type.find_page: acquire")?;
        let platform = current_platform_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_dict_type {DICT_TYPE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysDictType>(&rows_sql)
                .bind(platform.as_deref())
                .bind(filter.dict_name.as_deref())
                .bind(filter.dict_type.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "dict_type.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_dict_type {DICT_TYPE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(platform.as_deref())
                .bind(filter.dict_name.as_deref())
                .bind(filter.dict_type.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "dict_type.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(
                rows_ms,
                count_ms,
                total_ms,
                "dict_type.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Active dict types for dropdown — platform-scoped, capped at 500.
    #[instrument(skip_all)]
    pub async fn find_option_list(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysDictType>> {
        let platform = current_platform_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_dict_type \
             WHERE del_flag = '0' AND status = '0' \
               AND ($1::varchar IS NULL OR tenant_id = $1) \
             ORDER BY create_at DESC \
             LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysDictType>(&sql)
            .bind(platform.as_deref())
            .fetch_all(executor)
            .await
            .context("dict_type.find_option_list")?;
        Ok(rows)
    }

    #[instrument(skip_all, fields(dict_name = %params.dict_name))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: DictTypeInsertParams,
    ) -> anyhow::Result<SysDictType> {
        let audit = AuditInsert::now();
        let platform =
            current_platform_scope().context("dict_type.insert: platform_id required")?;
        let sql = format!(
            "INSERT INTO sys_dict_type (\
                dict_id, tenant_id, dict_name, dict_type, status, \
                del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, '0', $5, $6, \
                CURRENT_TIMESTAMP, $7\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysDictType>(&sql)
            .bind(&platform)
            .bind(&params.dict_name)
            .bind(&params.dict_type)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("dict_type.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(dict_id = %params.dict_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: DictTypeUpdateParams,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_dict_type SET \
                dict_name = COALESCE($3, dict_name), \
                dict_type = COALESCE($4, dict_type), \
                status    = COALESCE($5, status), \
                remark    = CASE WHEN $6::boolean THEN $7 ELSE remark END, \
                update_by = $8, \
                update_at = CURRENT_TIMESTAMP \
             WHERE dict_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(&params.dict_id)
        .bind(platform.as_deref())
        .bind(params.dict_name.as_deref())
        .bind(params.dict_type.as_deref())
        .bind(params.status.as_deref())
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("dict_type.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(dict_id = %dict_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        dict_id: &str,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_dict_type SET del_flag = '1', update_by = $3, update_at = CURRENT_TIMESTAMP \
             WHERE dict_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(dict_id)
        .bind(platform.as_deref())
        .bind(&update_by)
        .execute(executor)
        .await
        .context("dict_type.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}
