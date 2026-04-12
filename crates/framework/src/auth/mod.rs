//! Authentication primitives: JWT claims, Redis-backed user session, and
//! the route-level [`AccessSpec`] used by the RBAC middleware.

pub mod access_spec;
pub mod jwt;
pub mod session;

pub use access_spec::{AccessSpec, Role, Scope};
pub use jwt::JwtClaims;
pub use session::UserSession;
