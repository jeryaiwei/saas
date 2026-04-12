//! DictDataRepo — hand-written SQL for sys_dict_data.
//!
//! PLATFORM tenant model — filtered by platform_id.

use super::entities::SysDictData;
use anyhow::Context;
use framework::context::{audit_update_by, current_platform_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    dict_code, tenant_id, dict_sort, dict_label, dict_value, dict_type, \
    css_class, list_class, is_default, status, create_by, create_at, \
    update_by, update_at, remark, del_flag, i18n";

const DICT_DATA_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR dict_type = $2) \
      AND ($3::varchar IS NULL OR dict_label LIKE '%' || $3 || '%') \
      AND ($4::varchar IS NULL OR status = $4)";

#[derive(Debug)]
pub struct DictDataListFilter {
    pub dict_type: Option<String>,
    pub dict_label: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct DictDataInsertParams {
    pub dict_sort: i32,
    pub dict_label: String,
    pub dict_value: String,
    pub dict_type: String,
    pub css_class: String,
    pub list_class: String,
    pub is_default: String,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct DictDataUpdateParams {
    pub dict_code: String,
    pub dict_sort: Option<i32>,
    pub dict_label: Option<String>,
    pub dict_value: Option<String>,
    pub dict_type: Option<String>,
    pub css_class: Option<String>,
    pub list_class: Option<String>,
    pub is_default: Option<String>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct DictDataRepo;

impl DictDataRepo {
    #[instrument(skip_all, fields(dict_code = %dict_code))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        dict_code: &str,
    ) -> anyhow::Result<Option<SysDictData>> {
        let platform = current_platform_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_dict_data \
             WHERE dict_code = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysDictData>(&sql)
            .bind(dict_code)
            .bind(platform.as_deref())
            .fetch_optional(executor)
            .await
            .context("dict_data.find_by_id")?;
        Ok(row)
    }

    /// Find all dict data entries for a given dict_type (platform-scoped, capped at 500).
    #[instrument(skip_all, fields(dict_type = %dict_type))]
    pub async fn find_by_type(
        executor: impl sqlx::PgExecutor<'_>,
        dict_type: &str,
    ) -> anyhow::Result<Vec<SysDictData>> {
        let platform = current_platform_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_dict_data \
             WHERE dict_type = $1 AND del_flag = '0' AND status = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             ORDER BY dict_sort ASC \
             LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysDictData>(&sql)
            .bind(dict_type)
            .bind(platform.as_deref())
            .fetch_all(executor)
            .await
            .context("dict_data.find_by_type")?;
        Ok(rows)
    }

    /// Check if a (dict_type, dict_value) pair already exists.
    #[instrument(skip_all, fields(dict_type = %dict_type, dict_value = %dict_value))]
    pub async fn exists_by_type_value(
        executor: impl sqlx::PgExecutor<'_>,
        dict_type: &str,
        dict_value: &str,
        exclude_code: Option<&str>,
    ) -> anyhow::Result<bool> {
        let platform = current_platform_scope();
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_dict_data \
                WHERE dict_type = $1 AND dict_value = $2 AND del_flag = '0' \
                  AND ($3::varchar IS NULL OR tenant_id = $3) \
                  AND ($4::varchar IS NULL OR dict_code <> $4)\
            )",
        )
        .bind(dict_type)
        .bind(dict_value)
        .bind(platform.as_deref())
        .bind(exclude_code)
        .fetch_one(executor)
        .await
        .context("dict_data.exists_by_type_value")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(
        has_type = filter.dict_type.is_some(),
        has_label = filter.dict_label.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: DictDataListFilter,
    ) -> anyhow::Result<framework::response::Page<SysDictData>> {
        let mut conn = conn
            .acquire()
            .await
            .context("dict_data.find_page: acquire")?;
        let platform = current_platform_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_dict_data {DICT_DATA_PAGE_WHERE} \
             ORDER BY dict_sort ASC, create_at DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysDictData>(&rows_sql)
                .bind(platform.as_deref())
                .bind(filter.dict_type.as_deref())
                .bind(filter.dict_label.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "dict_data.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_dict_data {DICT_DATA_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(platform.as_deref())
                .bind(filter.dict_type.as_deref())
                .bind(filter.dict_label.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "dict_data.find_page count",
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
                "dict_data.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    #[instrument(skip_all, fields(dict_label = %params.dict_label))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: DictDataInsertParams,
    ) -> anyhow::Result<SysDictData> {
        let audit = AuditInsert::now();
        let platform =
            current_platform_scope().context("dict_data.insert: platform_id required")?;
        let sql = format!(
            "INSERT INTO sys_dict_data (\
                dict_code, tenant_id, dict_sort, dict_label, dict_value, dict_type, \
                css_class, list_class, is_default, status, del_flag, \
                create_by, update_by, update_at, remark\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, $5, $6, $7, $8, $9, '0', \
                $10, $11, CURRENT_TIMESTAMP, $12\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysDictData>(&sql)
            .bind(&platform)
            .bind(params.dict_sort)
            .bind(&params.dict_label)
            .bind(&params.dict_value)
            .bind(&params.dict_type)
            .bind(&params.css_class)
            .bind(&params.list_class)
            .bind(&params.is_default)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("dict_data.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(dict_code = %params.dict_code))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: DictDataUpdateParams,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_dict_data SET \
                dict_sort  = COALESCE($3, dict_sort), \
                dict_label = COALESCE($4, dict_label), \
                dict_value = COALESCE($5, dict_value), \
                dict_type  = COALESCE($6, dict_type), \
                css_class  = COALESCE($7, css_class), \
                list_class = COALESCE($8, list_class), \
                is_default = COALESCE($9, is_default), \
                status     = COALESCE($10, status), \
                remark     = CASE WHEN $11::boolean THEN $12 ELSE remark END, \
                update_by  = $13, \
                update_at  = CURRENT_TIMESTAMP \
             WHERE dict_code = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(&params.dict_code)
        .bind(platform.as_deref())
        .bind(params.dict_sort)
        .bind(params.dict_label.as_deref())
        .bind(params.dict_value.as_deref())
        .bind(params.dict_type.as_deref())
        .bind(params.css_class.as_deref())
        .bind(params.list_class.as_deref())
        .bind(params.is_default.as_deref())
        .bind(params.status.as_deref())
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("dict_data.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(dict_code = %dict_code))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        dict_code: &str,
    ) -> anyhow::Result<u64> {
        let platform = current_platform_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_dict_data SET del_flag = '1', update_by = $3, update_at = CURRENT_TIMESTAMP \
             WHERE dict_code = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(dict_code)
        .bind(platform.as_deref())
        .bind(&update_by)
        .execute(executor)
        .await
        .context("dict_data.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}
