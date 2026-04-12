//! Test helpers for downstream crates.
//!
//! This module is `pub` because cargo's test/integration layout makes
//! feature-gated test helpers painful to share across crates. The helpers
//! here are pure functions with zero runtime cost if unused, so they
//! ship in the prod binary without concern.
//!
//! See `docs/framework/framework-pagination-spec.md` §5 and v1.1 Phase for the
//! intended usage.

pub mod explain_plan;
pub mod pg_catalog;
