//! tracing-subscriber initialization.
//!
//! - Structured JSON to a daily-rotated file (always on).
//! - Stdout layer: JSON when `cfg.json = true`, otherwise human-friendly.
//! - Returns a [`TracingGuard`] that must be held for the lifetime of the
//!   process so the non-blocking file writer keeps flushing.

use crate::config::LoggerConfig;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

/// Drop this when the process exits to flush the file appender.
pub struct TracingGuard {
    _file_guard: WorkerGuard,
}

pub fn init(cfg: &LoggerConfig) -> TracingGuard {
    let filter = EnvFilter::try_new(&cfg.level).unwrap_or_else(|_| EnvFilter::new("info"));

    std::fs::create_dir_all(&cfg.dir).ok();
    let file_appender = tracing_appender::rolling::daily(&cfg.dir, "app.log");
    let (file_writer, file_guard) = tracing_appender::non_blocking(file_appender);

    let file_layer = fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false)
        .with_target(true)
        .json();

    // Two fully-typed branches avoid the `Box<dyn Layer<_>>` type-unification
    // pain of `tracing-subscriber`.
    if cfg.json {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(fmt::layer().with_target(true).json())
            .init();
    } else {
        tracing_subscriber::registry()
            .with(filter)
            .with(file_layer)
            .with(fmt::layer().with_target(true).with_ansi(true))
            .init();
    }

    TracingGuard {
        _file_guard: file_guard,
    }
}
