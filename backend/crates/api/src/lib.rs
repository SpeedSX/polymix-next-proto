pub mod auth;
pub mod config;
pub mod dev_issuer;
pub mod error;
pub mod jwks;
pub mod routes;
pub mod state;

use std::sync::Arc;

use axum::http::{header, Method};
use axum::{Router, middleware, routing::get, routing::post};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use config::AppConfig;
use dev_issuer::DevIssuer;
use jwks::JwksCache;
use state::AppState;
use surreal_store::{DbConfig, Store, TenantProvisioner};

pub async fn build_state(config: AppConfig) -> anyhow::Result<AppState> {
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

    Ok(AppState {
        config: Arc::new(config),
        store,
        provisioner,
        jwks,
        dev_issuer,
    })
}

pub fn build_router(state: AppState) -> Router {
    let mut router = Router::new().route("/api/health", get(routes::health::health));
    if state.config.auth_dev_mode {
        router = router
            .route("/dev/jwks.json", get(routes::dev::jwks))
            .route("/dev/token", post(routes::dev::token));
    }

    let protected = Router::new()
        .route("/api/me", get(routes::me::me))
        .route(
            "/api/customers",
            get(routes::customers::list).post(routes::customers::create),
        )
        .route(
            "/api/customers/{id}",
            get(routes::customers::get)
                .put(routes::customers::update)
                .delete(routes::customers::delete),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    router
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(Any)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE]),
        )
        .with_state(state)
}
