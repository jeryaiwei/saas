//! Custom Axum extractors. See [`ValidatedJson`] and [`ValidatedQuery`].

pub mod validated_json;
pub mod validated_query;

pub use validated_json::ValidatedJson;
pub use validated_query::ValidatedQuery;
