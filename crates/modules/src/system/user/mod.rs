//! User management endpoints — admin CRUD + management.
//! Personal profile + batch endpoints deferred to Sub-Phase 2b.

pub mod dto;
pub mod handler;
pub mod service;

pub use handler::router;
