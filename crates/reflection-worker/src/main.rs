use reflection_core::AppConfig;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "reflection_worker=debug".to_string()),
        )
        .compact()
        .init();

    let config = AppConfig::from_env()?;
    tracing::info!(
        storage_dir = %config.storage_dir.display(),
        "Reflection King standalone worker placeholder"
    );

    tracing::warn!(
        "standalone worker is not enabled yet; reflection-api currently owns local queue dispatch"
    );
    Ok(())
}
