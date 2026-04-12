//! Unified HTTP response envelope, pagination, and business response codes.

pub mod codes;
pub mod envelope;
pub mod pagination;
pub mod time;

#[cfg(test)]
mod wire_test;

pub use codes::ResponseCode;
pub use envelope::ApiResponse;
pub use pagination::{
    with_timeout, with_timeout_for, Page, PageQuery, PaginationParams, PAGE_NUM_DEFAULT,
    PAGE_NUM_MAX, PAGE_SIZE_DEFAULT, PAGE_SIZE_MAX, QUERY_TIMEOUT_SECS, SLOW_QUERY_WARN_MS,
};
pub use time::fmt_ts;
