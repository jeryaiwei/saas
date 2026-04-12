//! Auth module — `/auth/login`, `/auth/code`, `/auth/logout`, `/info`.

pub mod dto;
pub mod handler;
pub mod service;

pub use handler::router;
