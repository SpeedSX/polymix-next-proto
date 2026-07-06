mod auth;
mod config;
mod dev_issuer;
mod error;
mod jwks;
mod routes;
mod state;

use std::sync::Arc;

use axum::{Router, middleware, routing::get, routing::post};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use config::AppConfig;
use dev_issuer::DevIssuer;
use jwks::JwksCache;
use state::AppState;
use surreal_store::{DbConfig, Store, TenantProvisioner};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,api=debug".into()),
        )
        .init();

    let config = AppConfig::from_env()?;
    let port = config.port;
    tracing::info!(port, dev_mode = config.auth_dev_mode, "starting api");

    let store = Store::connect(&DbConfig {
        url: config.surrealdb_url.clone(),
        user: config.surrealdb_user.clone(),
        pass: config.surrealdb_pass.clone(),
        ns: config.surrealdb_ns.clone(),
    })
    .await?;
    let store = Arc::new(store);
    let provisioner = Arc::new(TenantProvisioner::new(store.clone()));
    let jwks = Arc::new(JwksCache::new(config.auth_jwks_url.clone()));
    let dev_issuer = if config.auth_dev_mode {
        Some(Arc::new(DevIssuer::generate()?))
    } else {
        None
    };

    let state = AppState {
        config: Arc::new(config.clone()),
        provisioner,
        jwks,
        dev_issuer,
    };

    let mut router = Router::new().route("/api/health", get(routes::health::health));
    if config.auth_dev_mode {
        router = router
            .route("/dev/jwks.json", get(routes::dev::jwks))
            .route("/dev/token", post(routes::dev::token));
    }

    let protected =
        Router::new()
            .route("/api/me", get(routes::me::me))
            .layer(middleware::from_fn_with_state(
                state.clone(),
                auth::require_auth,
            ));

    let app = router
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(CorsLayer::permissive())
        .with_state(state);

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
