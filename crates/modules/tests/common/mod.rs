//! Shared integration test scaffold. Loads `config/development.yaml`
//! via framework's `AppConfig::load`, builds a `PgPool` + `RedisPool`,
//! and exposes an `AppState` + an axum Router built from `modules::router`.
//!
//! These tests hit the live `saas_tea` dev database. Each test uses a
//! distinct `role_name` / `role_key` prefix (`it-{test_name}-`) and
//! cleans up its own rows on drop.

#![allow(dead_code)] // Some helpers used only by later batches.

use axum::Router;
use framework::{
    config::AppConfig,
    context::{scope, RequestContext},
    infra::{pg, redis},
};
use modules::{router, AppState};
use sqlx::PgPool;
use std::sync::{Arc, Once};

static INIT: Once = Once::new();

pub async fn build_state_and_router() -> (AppState, Router) {
    INIT.call_once(|| {
        let _ = tracing_subscriber::fmt()
            .with_test_writer()
            .with_env_filter("info")
            .try_init();

        // `AppConfig::load` resolves `config/{default,development}.yaml`
        // relative to the process CWD. `cargo test` launches the test
        // binary with CWD set to the crate dir (`crates/modules/`), not
        // the workspace root, so we `chdir` once to `server-rs/` — the
        // workspace root, two levels up from `CARGO_MANIFEST_DIR`.
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        if let Some(workspace_root) = manifest_dir.parent().and_then(|p| p.parent()) {
            let _ = std::env::set_current_dir(workspace_root);
        }
    });

    let cfg = Arc::new(AppConfig::load().expect("load config"));

    // Metrics recorder can only be installed once per process. Integration
    // tests run in a single process, so tolerate re-init errors by building
    // a throwaway recorder when the global is already set.
    let metrics_handle = framework::telemetry::metrics::init_recorder().unwrap_or_else(|_| {
        metrics_exporter_prometheus::PrometheusBuilder::new()
            .build_recorder()
            .handle()
    });

    let pg_pool = pg::connect_lazy(&cfg.db.postgresql).expect("pg pool");
    let redis_pool = redis::build(&cfg.db.redis).expect("redis pool");

    let state = AppState {
        config: cfg,
        pg: pg_pool,
        redis: redis_pool,
        metrics: metrics_handle,
    };
    let router = router(state.clone());
    (state, router)
}

/// Run `fut` inside a `RequestContext` matching the `admin / 000000`
/// super-admin. Skips the real JWT middleware because these tests call
/// repos directly or use the axum router.
pub async fn as_super_admin<F, T>(fut: F) -> T
where
    F: std::future::Future<Output = T>,
{
    let ctx = RequestContext {
        request_id: Some(format!("it-{}", uuid::Uuid::new_v4())),
        tenant_id: Some("000000".into()),
        platform_id: Some("000000".into()),
        user_id: Some("it-admin".into()),
        user_name: Some("it-admin".into()),
        user_type: Some("10".into()),
        is_admin: true,
        lang_code: Some("en-US".into()),
        ..Default::default()
    };
    scope(ctx, fut).await
}

/// Cleanup helper — delete all `sys_user` rows + their `sys_user_role`
/// and `sys_user_tenant` bindings created by a given test prefix.
/// Matches `user_name LIKE '{prefix}%'`. Used by user module integration
/// tests in Batch 11; compiled-in from Batch 2 for forward readiness.
pub async fn cleanup_test_users(pool: &PgPool, prefix: &str) {
    // Safety: empty prefix would match every user (LIKE '%') and wipe the
    // entire sys_user table. Test authors must use a discriminating prefix.
    assert!(
        !prefix.is_empty(),
        "cleanup_test_users: prefix must not be empty"
    );
    let pattern = format!("{prefix}%");
    // Order matters: delete join rows first (no audit fields), then the
    // user row itself. `.expect(...)` on each so cleanup failures surface
    // loudly in test output.
    sqlx::query(
        "DELETE FROM sys_user_role WHERE user_id IN \
         (SELECT user_id FROM sys_user WHERE user_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_role for test users");
    sqlx::query(
        "DELETE FROM sys_user_tenant WHERE user_id IN \
         (SELECT user_id FROM sys_user WHERE user_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_tenant for test users");
    sqlx::query("DELETE FROM sys_user WHERE user_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_user for test users");
}

/// Cleanup helper — delete all rows created by a given test prefix.
pub async fn cleanup_test_rows(pool: &PgPool, prefix: &str) {
    let pattern = format!("{prefix}%");
    sqlx::query(
        "DELETE FROM sys_role_menu WHERE role_id IN \
         (SELECT role_id FROM sys_role WHERE role_key LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_role_menu");
    sqlx::query(
        "DELETE FROM sys_user_role WHERE role_id IN \
         (SELECT role_id FROM sys_role WHERE role_key LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_role");
    sqlx::query("DELETE FROM sys_role WHERE role_key LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_role");
}

/// Cleanup helper — delete test tenants and their admin users/bindings.
/// Matches `sys_tenant.company_name LIKE '{prefix}%'` and deletes related
/// `sys_user_tenant` bindings and `sys_user` rows whose `user_name` matches
/// the same prefix.
pub async fn cleanup_test_tenants(pool: &PgPool, prefix: &str) {
    assert!(
        !prefix.is_empty(),
        "cleanup_test_tenants: prefix must not be empty"
    );
    let pattern = format!("{prefix}%");
    // Delete user-tenant bindings for users created by test tenants
    sqlx::query(
        "DELETE FROM sys_user_tenant WHERE user_id IN \
         (SELECT user_id FROM sys_user WHERE user_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_user_tenant for test tenant users");
    // Delete users created as tenant admins
    sqlx::query("DELETE FROM sys_user WHERE user_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_user for test tenant admins");
    // Delete test tenants
    sqlx::query("DELETE FROM sys_tenant WHERE company_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_tenant");
}

/// Cleanup helper — delete test packages by code prefix.
/// Matches `sys_tenant_package.code LIKE '{prefix}%'`.
pub async fn cleanup_test_packages(pool: &PgPool, prefix: &str) {
    assert!(
        !prefix.is_empty(),
        "cleanup_test_packages: prefix must not be empty"
    );
    let pattern = format!("{prefix}%");
    sqlx::query("DELETE FROM sys_tenant_package WHERE code LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_tenant_package");
}

/// Cleanup helper — delete test depts.
/// Matches `sys_dept.dept_name LIKE '{prefix}%'`.
pub async fn cleanup_test_depts(pool: &PgPool, prefix: &str) {
    assert!(
        !prefix.is_empty(),
        "cleanup_test_depts: prefix must not be empty"
    );
    let pattern = format!("{prefix}%");
    sqlx::query("DELETE FROM sys_dept WHERE dept_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_dept");
}

/// Cleanup helper — delete test menus and their `sys_role_menu` bindings.
/// Matches `sys_menu.menu_name LIKE '{prefix}%'`.
pub async fn cleanup_test_menus(pool: &PgPool, prefix: &str) {
    assert!(
        !prefix.is_empty(),
        "cleanup_test_menus: prefix must not be empty"
    );
    let pattern = format!("{prefix}%");
    sqlx::query(
        "DELETE FROM sys_role_menu WHERE menu_id IN \
         (SELECT menu_id FROM sys_menu WHERE menu_name LIKE $1)",
    )
    .bind(&pattern)
    .execute(pool)
    .await
    .expect("cleanup sys_role_menu for test menus");
    sqlx::query("DELETE FROM sys_menu WHERE menu_name LIKE $1")
        .bind(&pattern)
        .execute(pool)
        .await
        .expect("cleanup sys_menu");
}
