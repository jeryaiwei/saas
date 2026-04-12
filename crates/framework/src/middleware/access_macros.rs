//! Declarative macros that shortcut the verbose `route_layer(from_fn_with_state(...))`
//! pattern for permission/role/scope gates. Expand at call site into the
//! same code you would have written by hand — no proc macros, no extra
//! compile time, `cargo expand` shows the exact result.

/// Build a route-level layer that requires a specific permission string.
///
/// ```ignore
/// use framework::require_permission;
/// Router::new().route(
///     "/system/role/",
///     post(create).route_layer(require_permission!("system:role:add")),
/// )
/// ```
///
/// For combining a permission check with other gate types (sys_code,
/// scope, role) on the same route, use [`require_access!`] instead.
#[macro_export]
macro_rules! require_permission {
    ($perm:expr) => {
        ::axum::middleware::from_fn_with_state(
            $crate::middleware::access::require($crate::auth::AccessSpec::permission($perm)),
            $crate::middleware::access::enforce,
        )
    };
}

/// Build a route-level layer that requires a specific role
/// (`AccessSpec::role`). Takes a `framework::auth::Role` enum variant.
#[macro_export]
macro_rules! require_role {
    ($role:expr) => {
        ::axum::middleware::from_fn_with_state(
            $crate::middleware::access::require($crate::auth::AccessSpec::role($role)),
            $crate::middleware::access::enforce,
        )
    };
}

/// Build a route-level layer that requires a specific access scope
/// (`AccessSpec::scope`). Takes a `framework::auth::Scope` enum variant.
#[macro_export]
macro_rules! require_scope {
    ($scope:expr) => {
        ::axum::middleware::from_fn_with_state(
            $crate::middleware::access::require($crate::auth::AccessSpec::scope($scope)),
            $crate::middleware::access::enforce,
        )
    };
}

/// Route-layer macro for authenticated-only routes (no specific permission
/// or role required). Equivalent to the raw
/// `from_fn_with_state(access::require(AccessSpec::authenticated()), access::enforce)`
/// form.
///
/// Usage: `.route("/path", get(handler).route_layer(require_authenticated!()))`
#[macro_export]
macro_rules! require_authenticated {
    () => {
        ::axum::middleware::from_fn_with_state(
            $crate::middleware::access::require($crate::auth::AccessSpec::authenticated()),
            $crate::middleware::access::enforce,
        )
    };
}

/// Route-layer macro that composes **multiple** gate types (permission,
/// role, scope, sys_code) on a single route. Each field you list adds
/// an AND-connected check — the request must pass ALL of them.
///
/// The four single-gate macros (`require_permission!`, `require_role!`,
/// `require_scope!`, `require_authenticated!`) cover the 95% case where a
/// route has exactly ONE gate type. Reach for `require_access!` when a
/// route legitimately needs to combine gates — for example, a platform-
/// admin-only endpoint that's also subsystem-restricted.
///
/// ```ignore
/// use framework::require_access;
/// use framework::auth::{Role, Scope};
///
/// // permission + sys_code
/// .route_layer(require_access! {
///     permission: "system:user:list",
///     sys_code: "ADMIN",
/// })
///
/// // role + sys_code
/// .route_layer(require_access! {
///     role: Role::TenantAdmin,
///     sys_code: "PLATFORM",
/// })
///
/// // permission + scope + sys_code
/// .route_layer(require_access! {
///     permission: "trade:order:list",
///     scope: Scope::Shared,
///     sys_code: "SUPPLIER",
/// })
/// ```
///
/// **Supported field names** — the macro does not accept any other key:
/// - `permission: &str` — route-level permission string
/// - `role: framework::auth::Role` — enum variant
/// - `scope: framework::auth::Scope` — enum variant
/// - `sys_code: &str` — subsystem isolation code; pass the macro multiple
///   times OR chain multiple `sys_code:` entries to add more than one
///
/// Trailing comma is allowed. Fields can be listed in any order.
#[macro_export]
macro_rules! require_access {
    ( $($field:ident : $value:expr),+ $(,)? ) => {{
        #[allow(unused_mut)]
        let mut __spec = $crate::auth::AccessSpec::authenticated();
        $(
            $crate::__require_access_set!(__spec, $field, $value);
        )+
        ::axum::middleware::from_fn_with_state(
            $crate::middleware::access::require(__spec),
            $crate::middleware::access::enforce,
        )
    }};
}

/// Internal helper for [`require_access!`]. Dispatches a `field: value`
/// entry to the correct `AccessSpec::with_*` builder method. Any field
/// name not listed here is a compile error, which is the desired
/// behavior — typos at the call site should fail fast.
#[doc(hidden)]
#[macro_export]
macro_rules! __require_access_set {
    ($spec:ident, permission, $v:expr) => {
        $spec = $spec.with_permission($v);
    };
    ($spec:ident, role, $v:expr) => {
        $spec = $spec.with_role($v);
    };
    ($spec:ident, scope, $v:expr) => {
        $spec = $spec.with_scope($v);
    };
    ($spec:ident, sys_code, $v:expr) => {
        $spec = $spec.with_sys_code($v);
    };
}
