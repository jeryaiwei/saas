//! System (backend management) endpoints.
//!
//! Phase 1 sub-phase 1 adds only the `role` module. Subsequent sub-phases
//! add user, menu, dept, post, dict, config, tenant.

pub mod dept;
pub mod menu;
pub mod role;
pub mod tenant;
pub mod tenant_package;
pub mod user;
