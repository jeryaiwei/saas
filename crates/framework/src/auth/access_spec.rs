//! `AccessSpec` — route-level RBAC specification attached to protected routes.
//!
//! Usage at the call site (Gate 5 `modules/auth/handler.rs`):
//!
//! ```ignore
//! use axum::{routing::get, middleware::from_fn_with_state};
//! use framework::auth::AccessSpec;
//! use framework::middleware::access;
//!
//! Router::new().route(
//!     "/system/user/list",
//!     get(user_list).layer(from_fn_with_state(
//!         access::require(AccessSpec::permission("system:user:list")),
//!         access::enforce,
//!     )),
//! )
//! ```

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// `tenantId == "000000" && isAdmin`
    SuperAdmin,
    /// `tenantId == "000000"` regardless of admin flag
    SuperTenant,
    /// `tenantId == platformId && isAdmin`
    PlatformAdmin,
    /// `isAdmin` within the current tenant
    TenantAdmin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// C-end users only (`userType == "20"`).
    Client,
    /// Backend and C-end users (`"10"` or `"20"`).
    Shared,
}

#[derive(Debug, Default, Clone)]
pub struct AccessSpec {
    pub role: Option<Role>,
    pub permission: Option<String>,
    pub scope: Option<Scope>,
    pub sys_codes: Vec<String>,
}

impl AccessSpec {
    /// Any authenticated user.
    pub fn authenticated() -> Self {
        Self::default()
    }

    pub fn permission(perm: impl Into<String>) -> Self {
        Self {
            permission: Some(perm.into()),
            ..Default::default()
        }
    }

    pub fn role(role: Role) -> Self {
        Self {
            role: Some(role),
            ..Default::default()
        }
    }

    pub fn scope(scope: Scope) -> Self {
        Self {
            scope: Some(scope),
            ..Default::default()
        }
    }

    pub fn with_permission(mut self, perm: impl Into<String>) -> Self {
        self.permission = Some(perm.into());
        self
    }

    pub fn with_role(mut self, role: Role) -> Self {
        self.role = Some(role);
        self
    }

    pub fn with_scope(mut self, scope: Scope) -> Self {
        self.scope = Some(scope);
        self
    }

    pub fn with_sys_code(mut self, sys_code: impl Into<String>) -> Self {
        self.sys_codes.push(sys_code.into());
        self
    }
}
