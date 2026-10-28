use anyhow::Result;

pub mod evalexpr;

pub async fn init_metrics(namespace: &str) -> Result<()> {
    //check if we're running as a task in aws
    let u = std::env::var("ECS_CONTAINER_METADATA_URI")
        .map_err(|_| anyhow::anyhow!("Not logging metrics: Not running in AWS"))?;

    log::info!("Running in AWS");

    let j = reqwest::get(u)
        .await
        .map_err(|e| anyhow::anyhow!("Error fetching metadata: {e}"))?
        .json::<serde_json::Value>()
        .await
        .map_err(|e| anyhow::anyhow!("Error parsing metadata: {e}"))?;

    log::debug!("Metadata: {:?}", j);

    let m = j
        .as_object()
        .and_then(|v| v.get("Labels"))
        .and_then(|v| v.as_object())
        .ok_or_else(|| anyhow::anyhow!("No metrics found"))?;

    log::debug!("Metrics: {:?}", m);

    let c = m
        .get("com.amazonaws.ecs.task-definition-family")
        .and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("Cluster or task not found"))?;

    log::info!(
        "Setting up AWS logging with namespace {} and task family: {}",
        namespace,
        c
    );

    metrics_cloudwatch::Builder::new()
        .await
        .cloudwatch_namespace(namespace)
        .default_dimension("Task Family", c.to_string())
        .init_thread(metrics_cloudwatch::metrics::set_boxed_recorder)
        .map_err(|e| anyhow::anyhow!("Error initializing cloudwatch collector: {}", e))?;

    log::info!("Cloudwatch collector initialized");

    Ok(())
}
