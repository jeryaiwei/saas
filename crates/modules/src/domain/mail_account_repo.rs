//! MailAccountRepo — hand-written SQL for sys_mail_account.
//!
//! DAO conventions:
//! 1. Each method is one SQL statement.
//! 2. No cross-repo calls — only service.rs orchestrates.
//! 3. INSERT/UPDATE/DELETE on sys_mail_account are single-owned here.
//! 4. NOT tenant-scoped — no current_tenant_scope.

use super::entities::SysMailAccount;
use anyhow::Context;
use framework::context::{audit_update_by, AuditInsert};
use framework::response::{with_timeout, PageQuery, PaginationParams, SLOW_QUERY_WARN_MS};
use tracing::instrument;

const COLUMNS: &str = "\
    id, mail, username, password, host, port, ssl_enable, status, \
    remark, create_by, create_at, update_by, update_at, del_flag";

const PAGE_WHERE: &str = "\
    WHERE del_flag = '0' \
      AND ($1::varchar IS NULL OR mail LIKE '%' || $1 || '%') \
      AND ($2::varchar IS NULL OR status = $2)";

#[derive(Debug)]
pub struct MailAccountListFilter {
    pub mail: Option<String>,
    pub status: Option<String>,
    pub page: PageQuery,
}

#[derive(Debug)]
pub struct MailAccountInsertParams {
    pub mail: String,
    pub username: String,
    pub password: String,
    pub host: String,
    pub port: i32,
    pub ssl_enable: bool,
    pub status: String,
    pub remark: Option<String>,
}

#[derive(Debug)]
pub struct MailAccountUpdateParams {
    pub id: i32,
    pub mail: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
    pub host: Option<String>,
    pub port: Option<i32>,
    pub ssl_enable: Option<bool>,
    pub status: Option<String>,
    pub remark: Option<Option<String>>,
}

pub struct MailAccountRepo;

impl MailAccountRepo {
    #[instrument(skip_all, fields(id = %id))]
    pub async fn find_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        id: i32,
    ) -> anyhow::Result<Option<SysMailAccount>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_mail_account \
             WHERE id = $1 AND del_flag = '0' \
             LIMIT 1"
        );
        let row = sqlx::query_as::<_, SysMailAccount>(&sql)
            .bind(id)
            .fetch_optional(executor)
            .await
            .context("mail_account.find_by_id")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(
        has_mail = filter.mail.is_some(),
        has_status = filter.status.is_some(),
        page_num = filter.page.page_num,
        page_size = filter.page.page_size,
    ))]
    pub async fn find_page(
        conn: impl sqlx::Acquire<'_, Database = sqlx::Postgres>,
        filter: MailAccountListFilter,
    ) -> anyhow::Result<framework::response::Page<SysMailAccount>> {
        let mut conn = conn
            .acquire()
            .await
            .context("mail_account.find_page: acquire")?;
        let p = PaginationParams::from(filter.page.page_num, filter.page.page_size);

        let rows_sql = format!(
            "SELECT {COLUMNS} FROM sys_mail_account {PAGE_WHERE} \
             ORDER BY id DESC \
             LIMIT $3 OFFSET $4"
        );
        let rows_start = std::time::Instant::now();
        let rows = with_timeout(
            sqlx::query_as::<_, SysMailAccount>(&rows_sql)
                .bind(filter.mail.as_deref())
                .bind(filter.status.as_deref())
                .bind(p.limit)
                .bind(p.offset)
                .fetch_all(&mut *conn),
            "mail_account.find_page rows",
        )
        .await?;
        let rows_ms = rows_start.elapsed().as_millis();

        let count_sql = format!("SELECT COUNT(*) FROM sys_mail_account {PAGE_WHERE}");
        let count_start = std::time::Instant::now();
        let observed_total: i64 = with_timeout(
            sqlx::query_scalar(&count_sql)
                .bind(filter.mail.as_deref())
                .bind(filter.status.as_deref())
                .fetch_one(&mut *conn),
            "mail_account.find_page count",
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
                "mail_account.find_page: slow query"
            );
        }

        let total = PaginationParams::reconcile_total(observed_total, rows.len(), p.offset);
        Ok(p.into_page(rows, total))
    }

    /// Enabled accounts for dropdown — status = '0', capped at 500.
    #[instrument(skip_all)]
    pub async fn find_enabled_list(
        executor: impl sqlx::PgExecutor<'_>,
    ) -> anyhow::Result<Vec<SysMailAccount>> {
        let sql = format!(
            "SELECT {COLUMNS} FROM sys_mail_account \
             WHERE del_flag = '0' AND status = '0' \
             ORDER BY id DESC \
             LIMIT 500"
        );
        let rows = sqlx::query_as::<_, SysMailAccount>(&sql)
            .fetch_all(executor)
            .await
            .context("mail_account.find_enabled_list")?;
        Ok(rows)
    }

    /// Check if a mail already exists (excluding a given id for update scenarios).
    #[instrument(skip_all, fields(mail = %mail))]
    pub async fn exists_by_mail(
        executor: impl sqlx::PgExecutor<'_>,
        mail: &str,
        exclude_id: Option<i32>,
    ) -> anyhow::Result<bool> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(\
                SELECT 1 FROM sys_mail_account \
                WHERE mail = $1 AND del_flag = '0' \
                  AND ($2::int IS NULL OR id <> $2)\
            )",
        )
        .bind(mail)
        .bind(exclude_id)
        .fetch_one(executor)
        .await
        .context("mail_account.exists_by_mail")?;
        Ok(exists)
    }

    #[instrument(skip_all, fields(mail = %params.mail))]
    pub async fn insert(
        executor: impl sqlx::PgExecutor<'_>,
        params: MailAccountInsertParams,
    ) -> anyhow::Result<SysMailAccount> {
        let audit = AuditInsert::now();
        let sql = format!(
            "INSERT INTO sys_mail_account (\
                mail, username, password, host, port, ssl_enable, \
                status, del_flag, create_by, update_by, update_at, remark\
            ) VALUES (\
                $1, $2, $3, $4, $5, $6, $7, '0', $8, $9, \
                CURRENT_TIMESTAMP, $10\
            ) RETURNING {COLUMNS}"
        );
        let row = sqlx::query_as::<_, SysMailAccount>(&sql)
            .bind(&params.mail)
            .bind(&params.username)
            .bind(&params.password)
            .bind(&params.host)
            .bind(params.port)
            .bind(params.ssl_enable)
            .bind(&params.status)
            .bind(&audit.create_by)
            .bind(&audit.update_by)
            .bind(params.remark.as_deref())
            .fetch_one(executor)
            .await
            .context("mail_account.insert")?;
        Ok(row)
    }

    #[instrument(skip_all, fields(id = %params.id))]
    pub async fn update_by_id(
        executor: impl sqlx::PgExecutor<'_>,
        params: MailAccountUpdateParams,
    ) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_mail_account SET \
                mail       = COALESCE($2, mail), \
                username   = COALESCE($3, username), \
                password   = COALESCE($4, password), \
                host       = COALESCE($5, host), \
                port       = COALESCE($6, port), \
                ssl_enable = COALESCE($7, ssl_enable), \
                status     = COALESCE($8, status), \
                remark     = CASE WHEN $9::boolean THEN $10 ELSE remark END, \
                update_by  = $11, \
                update_at  = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(params.id)
        .bind(params.mail.as_deref())
        .bind(params.username.as_deref())
        .bind(params.password.as_deref())
        .bind(params.host.as_deref())
        .bind(params.port)
        .bind(params.ssl_enable)
        .bind(params.status.as_deref())
        // remark — nullable update via flag pattern
        .bind(params.remark.is_some())
        .bind(params.remark.as_ref().and_then(|o| o.as_deref()))
        .bind(&update_by)
        .execute(executor)
        .await
        .context("mail_account.update_by_id")?
        .rows_affected();
        Ok(rows)
    }

    #[instrument(skip_all, fields(id = %id))]
    pub async fn soft_delete(executor: impl sqlx::PgExecutor<'_>, id: i32) -> anyhow::Result<u64> {
        let update_by = audit_update_by();
        let rows = sqlx::query(
            "UPDATE sys_mail_account SET del_flag = '1', update_by = $2, update_at = CURRENT_TIMESTAMP \
             WHERE id = $1 AND del_flag = '0'",
        )
        .bind(id)
        .bind(&update_by)
        .execute(executor)
        .await
        .context("mail_account.soft_delete")?
        .rows_affected();
        Ok(rows)
    }
}
