//! System (backend management) endpoints.
//!
//! Phase 1 sub-phase 1 adds only the `role` module. Subsequent sub-phases
//! add user, menu, dept, post, dict, config, tenant.

pub mod config;
pub mod dept;
pub mod dict;
pub mod login_log;
pub mod menu;
pub mod notice;
pub mod oper_log;
pub mod post;
pub mod role;
pub mod tenant;
pub mod tenant_package;
pub mod user;
