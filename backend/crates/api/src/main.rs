use api::config::AppConfig;
use api::{build_router, build_state};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // reqwest's rustls-tls and surrealdb's wss:// engine each pull in a
    // different rustls crypto backend (aws-lc-rs / ring) — rustls 0.23 won't
    // guess between them, so pin one explicitly before any TLS connection is
    // opened (only exercised once SURREALDB_URL is wss://, e.g. SurrealDB Cloud).
    rustls::crypto::aws_lc_rs::default_provider()
        .install_default()
        .expect("no CryptoProvider installed yet");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,api=debug".into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    let port = config.port;
    tracing::info!(port, dev_mode = config.auth_dev_mode, "starting api");

    let state = build_state(config).await?;
    let app = build_router(state);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    tracing::info!(port, "listening");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    tracing::info!("shutdown signal received, draining in-flight requests");
}
