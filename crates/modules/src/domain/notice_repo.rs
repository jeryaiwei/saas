//! NoticeRepo — hand-written SQL for sys_notice.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_notice are single-owned here.
//! 4. STRICT tenant model — filtered by tenant_id.

use super::entities::SysNotice;
use anyhow::Context;
use framework::context::{audit_update_by, current_tenant_scope, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use sqlx::PgPool;
use tracing::instrument;

const COLUMNS: &str = "\
    notice_id, tenant_id, notice_title, notice_type, notice_content, \
    status, create_by, create_at, update_by, update_at, del_flag, remark";

const NOTICE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR notice_title LIKE '%' || $2 || '%') \
      AND ($3::varchar IS NULL OR notice_type = $3) \
      AND ($4::varchar IS NULL OR status = $4)";

#[derive(Debug)]
pub struct NoticeListFilter {
    pub notice_title: Option<String>,
    pub notice_type: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct NoticeInsertParams {
    pub notice_title: String,
    pub notice_type: String,
    pub notice_content: Option<String>,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct NoticeUpdateParams {
    pub notice_id: String,
    pub notice_title: Option<String>,
    pub notice_type: Option<String>,
    pub notice_content: Option<Option<String>>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct NoticeRepo;

impl NoticeRepo {
    #[instrument(skip_all, fields(notice_id = %notice_id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        notice_id: &str,
    ) -> anyhow::Result<Option<SysNotice>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_notice \
             WHERE notice_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysNotice>(&sql)
            .bind(notice_id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("notice.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_title = filter.notice_title.is_some(),
        has_type = filter.notice_type.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        pool: &PgPool,
        filter: NoticeListFilter,
    ) -> anyhow::Result<framework::response::Page<SysNotice>> {
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_notice {NOTICE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $5 OFFSET $6"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysNotice>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.notice_title.as_deref())
                .bind(filter.notice_type.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(pool),
            "notice.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_notice {NOTICE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.notice_title.as_deref())
                .bind(filter.notice_type.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(pool),
            "notice.find_page count",
        )
        .await?;
        let count_ms = count_start.elapsed().as_millis();

        let mut rows = rows;
        if rows.len() as i64 > p.limit {
            rows.truncate(p.limit as usize);
        }

        let total_ms = rows_ms + count_ms;
        if total_ms > SLOW_QUERY_WARN_MS {
            tracing::warn!(rows_ms, count_ms, total_ms, "notice.find_page: slow query");
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    #[instrument(skip_all, fields(notice_title = %params.notice_title))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: NoticeInsertParams,
    ) -> anyhow::Result<SysNotice> {
        let audit = AuditInsert::now();
        let tenant = current_tenant_scope().context("notice.insert: tenant_id required")?;
        let sql = format!(
            "INSERT INTO sys_notice (\
                notice_id, tenant_id, notice_title, notice_type, notice_content, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                gen_random_uuid(), $1, $2, $3, $4, $5, '0', $6, $7, \
                CURRENT_TIMESTAMP, $8\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysNotice>(&sql)
            .bind(&tenant)
            .bind(&params.notice_title)
            .bind(&params.notice_type)
            .bind(params.notice_content.as_deref())
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("notice.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(notice_id = %params.notice_id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: NoticeUpdateParams,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_notice SET \
                notice_title   = COALESCE($3, notice_title), \
                notice_type    = COALESCE($4, notice_type), \
                notice_content = CASE WHEN $5::boolean THEN $6 ELSE notice_content END, \
                status         = COALESCE($7, status), \
                remark         = CASE WHEN $8::boolean THEN $9 ELSE remark END, \
                update_by      = $10, \
                update_at      = CURRENT_TIMESTAMP \
             WHERE notice_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(&params.notice_id)
        .bind(tenant.as_deref())
        .bind(params.notice_title.as_deref())
        .bind(params.notice_type.as_deref())
        // notice_content — nullable update via flag pattern
        .bind(params.notice_content.is_some())
        .bind(params.notice_content.as_ref().and_then(|o| o.as_deref()))
        .bind(params.status.as_deref())
        // remark — nullable update via flag pattern
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("notice.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(notice_id = %notice_id))]
    pub async fn soft_delete(
        executor: impl sqlx::PgExecutor<'_>,
        notice_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_notice SET del_flag = '1', update_by = $3, update_at = CURRENT_TIMESTAMP \
             WHERE notice_id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(notice_id)
        .bind(tenant.as_deref())
        .bind(&update_by)
        .execute(executor)
        .await
        .context("notice.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}
