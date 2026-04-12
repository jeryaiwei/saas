//! Prometheus metrics exporter.
//!
//! `init_recorder()` installs a global `metrics` recorder and returns a handle
//! that can render the Prometheus text format. The `/metrics` HTTP handler in
//! `modules::health` will call `handle.render()`.

use metrics_exporter_prometheus::PrometheusBuilder;
pub use metrics_exporter_prometheus::PrometheusHandle;

pub fn init_recorder() -> anyhow::Result<PrometheusHandle> {
    let builder = PrometheusBuilder::new();
    let handle = builder
        .install_recorder()
        .map_err(|e| anyhow::anyhow!("install prometheus recorder: {e}"))?;
    Ok(handle)
}
