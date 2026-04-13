//! Per-request context stored in `tokio::task_local!`.
//!
//! Mirrors NestJS `TenantContext` (AsyncLocalStorage). Contains all
//! cross-cutting request state: request id, tenant/platform/user identity,
//! language, and the `ignore_tenant` override flag.
//!
//! ⚠️ Critical rule: `tokio::task_local!` does NOT propagate across
//! `tokio::spawn`. Use [`scope_spawn`] (not raw `tokio::spawn`) for any
//! background work that must preserve request context. See
//! `docs/plan` for the corresponding project-wide convention.

pub mod audit;

pub use audit::{audit_update_by, current_platform_scope, current_tenant_scope, AuditInsert};

use std::cell::RefCell;
use std::future::Future;

use crate::auth::Role;
use crate::constants::SUPER_TENANT_ID;

#[derive(Debug, Clone, Default)]
pub struct RequestContext {
    pub request_id: Option<String>,
    pub tenant_id: Option<String>,
    pub platform_id: Option<String>,
    pub sys_code: Option<String>,
    pub user_id: Option<String>,
    pub user_name: Option<String>,
    /// "10" = CUSTOM (backend) / "20" = CLIENT (C-end). Matches NestJS `SYS_USER_TYPE`.
    pub user_type: Option<String>,
    pub is_admin: bool,
    pub lang_code: Option<String>,
    pub ignore_tenant: bool,
}

tokio::task_local! {
    static CURRENT_CTX: RefCell<RequestContext>;
}

impl RequestContext {
    /// Clone the current context if any, otherwise return default.
    pub fn current_cloned() -> RequestContext {
        CURRENT_CTX
            .try_with(|c| c.borrow().clone())
            .unwrap_or_default()
    }

    /// Derive the highest role from current context fields.
    pub fn get_role(&self) -> Option<Role> {
        if self.is_admin && self.tenant_id.as_deref() == Some(SUPER_TENANT_ID) {
            Some(Role::SuperAdmin)
        } else if self.tenant_id.as_deref() == Some(SUPER_TENANT_ID) {
            Some(Role::SuperTenant)
        } else if self.is_admin && self.tenant_id == self.platform_id {
            Some(Role::PlatformAdmin)
        } else if self.is_admin {
            Some(Role::TenantAdmin)
        } else {
            None
        }
    }

    /// Run a closure with an immutable reference to the current context.
    /// Returns `None` if no context is in scope.
    pub fn with_current<F, R>(f: F) -> Option<R>
    where
        F: FnOnce(&RequestContext) -> R,
    {
        CURRENT_CTX.try_with(|c| f(&c.borrow())).ok()
    }

    /// Mutate the current context in place. No-op if no context is in scope.
    pub fn mutate<F>(f: F)
    where
        F: FnOnce(&mut RequestContext),
    {
        let _ = CURRENT_CTX.try_with(|c| f(&mut c.borrow_mut()));
    }
}

/// Enter a new context scope for `fut`.
pub async fn scope<F>(ctx: RequestContext, fut: F) -> F::Output
where
    F: Future,
{
    CURRENT_CTX.scope(RefCell::new(ctx), fut).await
}

/// Like `tokio::spawn` but clones the current `RequestContext` into the new
/// task so tenant/user/request-id propagate across the spawn boundary.
pub fn scope_spawn<F>(fut: F) -> tokio::task::JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    let ctx = RequestContext::current_cloned();
    tokio::spawn(async move { CURRENT_CTX.scope(RefCell::new(ctx), fut).await })
}

/// Temporarily override the tenant id for `fut`.
pub async fn run_with_tenant<F>(tenant_id: impl Into<String>, fut: F) -> F::Output
where
    F: Future,
{
    let mut ctx = RequestContext::current_cloned();
    ctx.tenant_id = Some(tenant_id.into());
    scope(ctx, fut).await
}

/// Temporarily disable tenant filtering for `fut` (mirrors NestJS
/// `TenantContext.runIgnoringTenant`).
pub async fn run_ignoring_tenant<F>(fut: F) -> F::Output
where
    F: Future,
{
    let mut ctx = RequestContext::current_cloned();
    ctx.ignore_tenant = true;
    scope(ctx, fut).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn scope_propagates_ctx() {
        let ctx = RequestContext {
            tenant_id: Some("t1".into()),
            user_id: Some("u1".into()),
            ..Default::default()
        };

        scope(ctx, async {
            let got = RequestContext::current_cloned();
            assert_eq!(got.tenant_id.as_deref(), Some("t1"));
            assert_eq!(got.user_id.as_deref(), Some("u1"));
        })
        .await;
    }

    #[tokio::test]
    async fn mutate_is_visible_within_scope() {
        scope(RequestContext::default(), async {
            RequestContext::mutate(|c| c.user_id = Some("u42".into()));
            let got = RequestContext::with_current(|c| c.user_id.clone()).flatten();
            assert_eq!(got.as_deref(), Some("u42"));
        })
        .await;
    }

    #[tokio::test]
    async fn run_with_tenant_overrides() {
        let ctx = RequestContext {
            tenant_id: Some("original".into()),
            ..Default::default()
        };
        scope(ctx, async {
            run_with_tenant("override", async {
                let got = RequestContext::current_cloned();
                assert_eq!(got.tenant_id.as_deref(), Some("override"));
            })
            .await;
            // Outer scope is unaffected.
            let got = RequestContext::current_cloned();
            assert_eq!(got.tenant_id.as_deref(), Some("original"));
        })
        .await;
    }

    #[tokio::test]
    async fn scope_spawn_preserves_ctx() {
        let ctx = RequestContext {
            request_id: Some("req-abc".into()),
            ..Default::default()
        };
        scope(ctx, async {
            let handle = scope_spawn(async { RequestContext::current_cloned().request_id });
            let got = handle.await.unwrap();
            assert_eq!(got.as_deref(), Some("req-abc"));
        })
        .await;
    }

    #[tokio::test]
    async fn bare_tokio_spawn_loses_ctx() {
        // Documents the hazard that `scope_spawn` exists to prevent.
        let ctx = RequestContext {
            request_id: Some("req-xyz".into()),
            ..Default::default()
        };
        scope(ctx, async {
            let handle = tokio::spawn(async { RequestContext::current_cloned().request_id });
            let got = handle.await.unwrap();
            assert!(got.is_none(), "bare tokio::spawn must not inherit ctx");
        })
        .await;
    }
}
