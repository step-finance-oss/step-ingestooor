use axum::Extension;
use metrics_exporter_prometheus::PrometheusHandle;

// rust-analyzer bug which thinks this is unused
#[allow(dead_code)]
pub async fn handle_metrics(Extension(handle): Extension<PrometheusHandle>) -> String {
    handle.render()
}
