use anyhow::Result;
use axum::{routing::get, Extension, Router};
use tokio::net::TcpListener;

use crate::axum::routes::metrics::handle_metrics;
use metrics_exporter_prometheus::PrometheusHandle;

// rust-analyzer bug which thinks this is unused
#[allow(dead_code)]
pub async fn run_axum_server(prometheus_handle: PrometheusHandle) -> Result<()> {
    let app = Router::new()
        .route("/metrics", get(handle_metrics))
        .layer(Extension(prometheus_handle));

    let listener = TcpListener::bind("0.0.0.0:3000").await?;
    axum::serve(listener, app).await?;

    Ok(())
}
