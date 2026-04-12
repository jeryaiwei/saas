//! Audit-field and tenant-scope helpers that read from the current
//! `RequestContext`. These live in `framework/` rather than in a
//! module-specific `common.rs` because they are pure framework glue —
//! they know nothing about domain entities, only how to ask the
//! `RequestContext` task-local for identity + tenant state.
//!
//! All three helpers are single-callsite readers: no trait hierarchy,
//! no macros, no allocations beyond the String clones required to
//! escape the `&RequestContext` borrow.

use super::RequestContext;

/// Audit values for INSERTs. Reads the current user id from the active
/// `RequestContext`; falls back to empty string for background tasks /
/// system-initiated writes without an HTTP caller.
pub struct AuditInsert {
    /// User id to stamp on `create_by`. Empty string means "no HTTP
    /// caller / system-initiated write".
    pub create_by: String,
    /// User id to stamp on `update_by`. Same semantics as `create_by`.
    pub update_by: String,
}

impl AuditInsert {
    /// Build audit values from the current `RequestContext`. Both
    /// `create_by` and `update_by` are populated with the caller's
    /// `user_id`, or the empty string if no HTTP caller is in scope
    /// (e.g. system-initiated writes, background tasks).
    pub fn now() -> Self {
        let user_id = RequestContext::with_current(|c| c.user_id.clone())
            .flatten()
            .unwrap_or_default();
        Self {
            create_by: user_id.clone(),
            update_by: user_id,
        }
    }
}

/// Audit value for UPDATEs — just the caller's user id.
pub fn audit_update_by() -> String {
    RequestContext::with_current(|c| c.user_id.clone())
        .flatten()
        .unwrap_or_default()
}

/// Current tenant id for STRICT-scoped queries. Returns `None` for super
/// tenant, when `run_ignoring_tenant` is in effect, or when no context is
/// in scope (e.g. unit tests without `scope`).
pub fn current_tenant_scope() -> Option<String> {
    RequestContext::with_current(|c| {
        if c.ignore_tenant {
            None
        } else {
            c.tenant_id.clone()
        }
    })
    .flatten()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::scope;

    #[tokio::test]
    async fn audit_insert_reads_current_user() {
        let ctx = RequestContext {
            user_id: Some("u-1".into()),
            ..Default::default()
        };
        scope(ctx, async {
            let a = AuditInsert::now();
            assert_eq!(a.create_by, "u-1");
            assert_eq!(a.update_by, "u-1");
        })
        .await;
    }

    #[tokio::test]
    async fn audit_insert_empty_when_no_user() {
        let ctx = RequestContext::default();
        scope(ctx, async {
            let a = AuditInsert::now();
            assert_eq!(a.create_by, "");
        })
        .await;
    }

    #[tokio::test]
    async fn audit_update_by_reads_current_user() {
        let ctx = RequestContext {
            user_id: Some("u-42".into()),
            ..Default::default()
        };
        scope(ctx, async {
            assert_eq!(audit_update_by(), "u-42");
        })
        .await;
    }

    #[tokio::test]
    async fn current_tenant_scope_returns_tenant() {
        let ctx = RequestContext {
            tenant_id: Some("t-1".into()),
            ..Default::default()
        };
        scope(ctx, async {
            assert_eq!(current_tenant_scope().as_deref(), Some("t-1"));
        })
        .await;
    }

    #[tokio::test]
    async fn current_tenant_scope_returns_none_when_ignoring() {
        let ctx = RequestContext {
            tenant_id: Some("t-1".into()),
            ignore_tenant: true,
            ..Default::default()
        };
        scope(ctx, async {
            assert_eq!(current_tenant_scope(), None);
        })
        .await;
    }

    #[tokio::test]
    async fn current_tenant_scope_returns_none_without_context() {
        assert_eq!(current_tenant_scope(), None);
    }
}
