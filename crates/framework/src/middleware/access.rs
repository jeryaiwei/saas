//! `access` — route-level RBAC enforcement.
//!
//! Attached per-route with `axum::middleware::from_fn_with_state`:
//!
//! ```ignore
//! use axum::{routing::get, middleware::from_fn_with_state};
//! use framework::auth::{AccessSpec, Role, Scope};
//! use framework::middleware::access;
//!
//! Router::new()
//!     .route(
//!         "/system/user/list",
//!         get(user_list).layer(from_fn_with_state(
//!             access::require(AccessSpec::permission("system:user:list")),
//!             access::enforce,
//!         )),
//!     )
//!     .route(
//!         "/auth/login",
//!         post(login),  // no layer = public (still subject to auth whitelist)
//!     );
//! ```
//!
//! Why route-level and not global: Axum global middleware runs **before**
//! per-route extensions are attached to the request, so reading an
//! `Extension<AccessSpec>` from inside a global layer never works. Route-level
//! `from_fn_with_state` wraps the single route, so the spec is in scope when
//! `enforce` runs.
//!
//! The match order is identical to the NestJS `AccessGuard`:
//! `scope` → `role` → `permission` → `sys_codes`.

use crate::auth::access_spec::{AccessSpec, Role, Scope};
use crate::auth::session::UserSession;
use crate::constants::{SUPER_TENANT_ID, USER_TYPE_CLIENT, USER_TYPE_CUSTOM};
use crate::error::AppError;
use crate::response::ResponseCode;
use axum::{
    extract::{Request, State},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

/// Build the state argument for [`enforce`]. See module docs for usage.
pub fn require(spec: AccessSpec) -> Arc<AccessSpec> {
    Arc::new(spec)
}

#[tracing::instrument(skip_all, name = "middleware.access", fields(
    has_permission = spec.permission.is_some(),
    has_role = spec.role.is_some(),
    has_scope = spec.scope.is_some(),
))]
pub async fn enforce(
    State(spec): State<Arc<AccessSpec>>,
    req: Request,
    next: Next,
) -> Result<Response, AppError> {
    let session = req
        .extensions()
        .get::<UserSession>()
        .cloned()
        .ok_or_else(|| AppError::auth(ResponseCode::TOKEN_INVALID))?;

    check(&spec, &session)?;
    Ok(next.run(req).await)
}

fn check(spec: &AccessSpec, session: &UserSession) -> Result<(), AppError> {
    // 1. Scope
    if let Some(scope) = spec.scope {
        let ok = match scope {
            Scope::Client => session.user_type == USER_TYPE_CLIENT,
            Scope::Shared => {
                session.user_type == USER_TYPE_CUSTOM || session.user_type == USER_TYPE_CLIENT
            }
        };
        if !ok {
            return Err(AppError::forbidden(ResponseCode::FORBIDDEN));
        }
    }

    // 2. Role
    if let Some(role) = spec.role {
        let ok = match role {
            Role::SuperAdmin => {
                session.is_admin && session.tenant_id.as_deref() == Some(SUPER_TENANT_ID)
            }
            Role::SuperTenant => session.tenant_id.as_deref() == Some(SUPER_TENANT_ID),
            Role::PlatformAdmin => session.is_admin && session.tenant_id == session.platform_id,
            Role::TenantAdmin => session.is_admin,
        };
        if !ok {
            return Err(AppError::forbidden(ResponseCode::FORBIDDEN));
        }
    }

    // 3. Permission
    if let Some(perm) = &spec.permission {
        if !session.permissions.iter().any(|p| p == perm) {
            return Err(AppError::forbidden(ResponseCode::FORBIDDEN));
        }
    }

    // 4. Sys code (subsystem isolation)
    if !spec.sys_codes.is_empty() {
        let matches = session
            .sys_code
            .as_deref()
            .map(|sc| spec.sys_codes.iter().any(|code| code == sc))
            .unwrap_or(false);
        if !matches {
            return Err(AppError::forbidden(ResponseCode::FORBIDDEN));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend_admin() -> UserSession {
        UserSession {
            user_id: "u".into(),
            user_name: "n".into(),
            user_type: "10".into(),
            tenant_id: Some("t0".into()),
            platform_id: Some("t0".into()),
            is_admin: true,
            permissions: vec!["system:user:list".into()],
            ..Default::default()
        }
    }

    #[test]
    fn authenticated_only_passes() {
        assert!(check(&AccessSpec::authenticated(), &backend_admin()).is_ok());
    }

    #[test]
    fn permission_match() {
        let spec = AccessSpec::permission("system:user:list");
        assert!(check(&spec, &backend_admin()).is_ok());
    }

    #[test]
    fn permission_miss_forbidden() {
        let spec = AccessSpec::permission("system:role:list");
        assert!(check(&spec, &backend_admin()).is_err());
    }

    #[test]
    fn scope_client_rejects_backend() {
        let spec = AccessSpec::scope(Scope::Client);
        assert!(check(&spec, &backend_admin()).is_err());
    }

    #[test]
    fn scope_shared_allows_both() {
        let spec = AccessSpec::scope(Scope::Shared);
        let mut s = backend_admin();
        assert!(check(&spec, &s).is_ok());
        s.user_type = "20".into();
        assert!(check(&spec, &s).is_ok());
    }

    #[test]
    fn super_admin_requires_tenant_000000() {
        let spec = AccessSpec::role(Role::SuperAdmin);
        let mut s = backend_admin();
        assert!(check(&spec, &s).is_err());
        s.tenant_id = Some("000000".into());
        assert!(check(&spec, &s).is_ok());
    }

    #[test]
    fn tenant_admin_requires_is_admin() {
        let spec = AccessSpec::role(Role::TenantAdmin);
        let mut s = backend_admin();
        assert!(check(&spec, &s).is_ok());
        s.is_admin = false;
        assert!(check(&spec, &s).is_err());
    }

    #[test]
    fn sys_codes_match_or_fail() {
        let spec = AccessSpec::authenticated().with_sys_code("ADMIN");
        let mut s = backend_admin();
        assert!(check(&spec, &s).is_err()); // sys_code None
        s.sys_code = Some("ADMIN".into());
        assert!(check(&spec, &s).is_ok());
        s.sys_code = Some("OTHER".into());
        assert!(check(&spec, &s).is_err());
    }

    // === Combined gate tests ===
    //
    // These cover the AND semantics when `AccessSpec` carries more than
    // one gate type simultaneously (permission + sys_code, role + scope,
    // etc.). The `require_access!` macro builds specs of this shape, so
    // these tests also pin down the macro's runtime contract.

    #[test]
    fn permission_and_sys_code_both_required() {
        // Spec requires permission X AND sys_code Y. A session with only
        // the permission but wrong sys_code must be rejected.
        let spec = AccessSpec::permission("system:user:list").with_sys_code("ADMIN");
        let mut s = backend_admin();
        // Has the permission but no sys_code — sys_code gate fails.
        assert!(check(&spec, &s).is_err());
        // Add matching sys_code — now both gates pass.
        s.sys_code = Some("ADMIN".into());
        assert!(check(&spec, &s).is_ok());
        // Different sys_code — sys_code gate fails again.
        s.sys_code = Some("OTHER".into());
        assert!(check(&spec, &s).is_err());
    }

    #[test]
    fn role_and_sys_code_both_required() {
        // TenantAdmin role AND ADMIN sys_code.
        let spec = AccessSpec::role(Role::TenantAdmin).with_sys_code("ADMIN");
        let mut s = backend_admin();
        // is_admin=true satisfies role, but no sys_code yet → fail.
        assert!(check(&spec, &s).is_err());
        s.sys_code = Some("ADMIN".into());
        assert!(check(&spec, &s).is_ok());
        // Drop admin → role gate fails even with sys_code.
        s.is_admin = false;
        assert!(check(&spec, &s).is_err());
    }

    #[test]
    fn permission_role_and_scope_all_required() {
        // The most combined case: permission + role + scope all set.
        let spec = AccessSpec::permission("system:user:list")
            .with_role(Role::TenantAdmin)
            .with_scope(Scope::Shared);
        let s = backend_admin();
        // backend_admin has is_admin=true, user_type="10", and the right
        // permission → all three gates pass.
        assert!(check(&spec, &s).is_ok());
    }
}
