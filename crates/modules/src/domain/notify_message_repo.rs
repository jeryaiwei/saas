//! NotifyMessageRepo — hand-written SQL for sys_notify_message.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_notify_message are single-owned here.
//! 4. STRICT tenant model — filtered by tenant_id.

use super::entities::SysNotifyMessage;
use anyhow::Context;
use framework::context::current_tenant_scope;
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, tenant_id, user_id, user_type, template_id, template_code, \
    template_nickname, template_content, template_params, read_status, \
    read_time, del_flag, create_at, update_at";

const MESSAGE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND ($2::varchar IS NULL OR template_code = $2) \
      AND ($3::varchar IS NULL OR user_id = $3)";

const MY_MESSAGE_PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR tenant_id = $1) \
      AND user_id = $2 \
      AND ($3::boolean IS NULL OR read_status = $3)";

#[derive(Debug)]
pub struct NotifyMessageListFilter {
    pub template_code: Option<String>,
    pub user_id: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct NotifyMyMessageFilter {
    pub read_status: Option<bool>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct NotifyMessageInsertParams {
    pub user_id: String,
    pub user_type: i32,
    pub template_id: i32,
    pub template_code: String,
    pub template_nickname: String,
    pub template_content: String,
    pub template_params: Option<String>,
}

pub struct NotifyMessageRepo;

impl NotifyMessageRepo {
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i64,
    ) -> anyhow::Result<Option<SysNotifyMessage>> {
        let tenant = current_tenant_scope();
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_notify_message \
             WHERE id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2) \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysNotifyMessage>(&sql)
            .bind(id)
            .bind(tenant.as_deref())
            .fetch_optional(executor)
            .await
            .context("notify_message.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_template_code = filter.template_code.is_some(),
        has_user_id = filter.user_id.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: NotifyMessageListFilter,
    ) -> anyhow::Result<framework::response::Page<SysNotifyMessage>> {
        let mut conn = conn
            .acquire()
            .await
            .context("notify_message.find_page: acquire")?;
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_notify_message {MESSAGE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysNotifyMessage>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(filter.template_code.as_deref())
                .bind(filter.user_id.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "notify_message.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_notify_message {MESSAGE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(filter.template_code.as_deref())
                .bind(filter.user_id.as_deref())
                .fetch_one(&mut *conn),
            "notify_message.find_page count",
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
                "notify_message.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    #[instrument(skip_all, fields(
        user_id = %user_id,
        has_read_status = filter.read_status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_my_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        user_id: &str,
        filter: NotifyMyMessageFilter,
    ) -> anyhow::Result<framework::response::Page<SysNotifyMessage>> {
        let mut conn = conn
            .acquire()
            .await
            .context("notify_message.find_my_page: acquire")?;
        let tenant = current_tenant_scope();
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_notify_message {MY_MESSAGE_PAGE_WHERE} \
             ORDER BY create_at DESC \
             LIMIT $4 OFFSET $5"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysNotifyMessage>(&rows_sql)
                .bind(tenant.as_deref())
                .bind(user_id)
                .bind(filter.read_status)
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "notify_message.find_my_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_notify_message {MY_MESSAGE_PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(tenant.as_deref())
                .bind(user_id)
                .bind(filter.read_status)
                .fetch_one(&mut *conn),
            "notify_message.find_my_page count",
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
                "notify_message.find_my_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    #[instrument(skip_all, fields(user_id = %user_id))]
    pub async fn count_unread(
        executor: impl sqlx::PgExecutor<'_>,
        user_id: &str,
    ) -> anyhow::Result<i64> {
        let tenant = current_tenant_scope();
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sys_notify_message \
             WHERE del_flag = '0' AND read_status = false \
               AND user_id = $1 \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(user_id)
        .bind(tenant.as_deref())
        .fetch_one(executor)
        .await
        .context("notify_message.count_unread")?;
        Ok(count)
    }

    #[instrument(skip_all, fields(user_id = %params.user_id))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: NotifyMessageInsertParams,
    ) -> anyhow::Result<SysNotifyMessage> {
        let tenant = current_tenant_scope().context("notify_message.insert: tenant_id required")?;
        let sql = format!(
            "INSERT INTO sys_notify_message (\
                tenant_id, user_id, user_type, template_id, template_code, \
                template_nickname, template_content, template_params, \
                read_status, del_flag\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, $7, $8, false, '0'\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysNotifyMessage>(&sql)
            .bind(&tenant)
            .bind(&params.user_id)
            .bind(params.user_type)
            .bind(params.template_id)
            .bind(&params.template_code)
            .bind(&params.template_nickname)
            .bind(&params.template_content)
            .bind(params.template_params.as_deref())
            .fetch_one(executor)
            .await
            .context("notify_message.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn mark_read(executor: impl sqlx::PgExecutor<'_>, id: i64) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let rows = sqlx::query(
            "UPDATE sys_notify_message SET \
                read_status = true, \
                read_time = CURRENT_TIMESTAMP, \
                update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("notify_message.mark_read")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(user_id = %user_id))]
    pub async fn mark_all_read(
        executor: impl sqlx::PgExecutor<'_>,
        user_id: &str,
    ) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let rows = sqlx::query(
            "UPDATE sys_notify_message SET \
                read_status = true, \
                read_time = CURRENT_TIMESTAMP, \
                update_at = CURRENT_TIMESTAMP \
             WHERE user_id = $1 AND del_flag = '0' AND read_status = false \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(user_id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("notify_message.mark_all_read")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn soft_delete(executor: impl sqlx::PgExecutor<'_>, id: i64) -> anyhow::Result<u64> {
        let tenant = current_tenant_scope();
        let rows = sqlx::query(
            "UPDATE sys_notify_message SET del_flag = '1', update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0' \
               AND ($2::varchar IS NULL OR tenant_id = $2)",
        )
        .bind(id)
        .bind(tenant.as_deref())
        .execute(executor)
        .await
        .context("notify_message.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}
