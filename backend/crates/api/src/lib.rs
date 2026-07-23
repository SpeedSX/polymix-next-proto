pub mod auth;
pub mod backend;
pub mod config;
pub mod dev_issuer;
pub mod error;
pub mod jwks;
pub mod publisher;
pub mod routes;
pub mod state;
pub mod ws;

use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderValue, Method, header};
use axum::{Router, middleware, routing::get, routing::post};
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use tower_http::trace::TraceLayer;

use backend::SurrealBackend;
use config::AppConfig;
use dev_issuer::DevIssuer;
use jwks::JwksCache;
use publisher::NoopPublisher;
use state::AppState;
use surreal_store::{DbConfig, Store};

pub async fn build_state(config: AppConfig) -> anyhow::Result<AppState> {
    let store = Store::connect(&DbConfig {
        url: config.surrealdb_url.clone(),
        user: config.surrealdb_user.clone(),
        pass: config.surrealdb_pass.clone(),
        ns: config.surrealdb_ns.clone(),
    })
    .await?;
    let store = Arc::new(store);
    let backend = Arc::new(SurrealBackend::new(store.clone()).await?);
    let jwks = Arc::new(JwksCache::new(config.auth_jwks_url.clone()));
    let dev_issuer = if config.auth_dev_mode {
        Some(Arc::new(DevIssuer::generate()?))
    } else {
        None
    };

    let hub = Arc::new(ws::hub::Hub::new(store.clone()));

    Ok(AppState {
        config: Arc::new(config),
        backend,
        publisher: Arc::new(NoopPublisher),
        jwks,
        dev_issuer,
        hub,
    })
}

pub fn build_router(state: AppState) -> Router {
    // `/api/ws` sits outside the `require_auth` layer: it authenticates
    // itself from the `?token=` query parameter (see `ws::handler`).
    let mut router = Router::new()
        .route("/api/health", get(routes::health::health))
        .route("/api/ready", get(routes::health::ready))
        .route("/api/ws", get(ws::handler::ws));
    if state.config.auth_dev_mode {
        router = router
            .route("/dev/jwks.json", get(routes::dev::jwks))
            .route("/dev/token", post(routes::dev::token));
    }

    let protected = Router::new()
        .route("/api/me", get(routes::me::me))
        .route(
            "/api/dictionaries/order-statuses",
            get(routes::dictionaries::order_statuses),
        )
        .route(
            "/api/dictionaries/customer-statuses",
            get(routes::dictionaries::customer_statuses),
        )
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
        .route(
            "/api/customers/{id}/status",
            post(routes::customers::set_status),
        )
        .route(
            "/api/customers/{id}/activity",
            get(routes::orders::customer_activity),
        )
        .route(
            "/api/orders",
            get(routes::orders::list).post(routes::orders::create),
        )
        .route(
            "/api/orders/{id}",
            get(routes::orders::get)
                .put(routes::orders::update)
                .delete(routes::orders::delete),
        )
        .route("/api/orders/{id}/status", post(routes::orders::set_status))
        .route(
            "/api/orders/{id}/invoice",
            post(routes::invoices::create_from_order),
        )
        .route(
            "/api/invoices",
            get(routes::invoices::list).post(routes::invoices::create),
        )
        .route(
            "/api/invoices/{id}",
            get(routes::invoices::get)
                .put(routes::invoices::update)
                .delete(routes::invoices::delete),
        )
        .route(
            "/api/invoices/{id}/status",
            post(routes::invoices::set_status),
        )
        .route("/api/search", get(routes::search::search))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth::require_auth,
        ));

    // `/api/ws` is unaffected by any of this: browsers do not enforce CORS on
    // WebSocket upgrades, so its protection is the JWT in the query string,
    // nothing else.
    let allow_origin = match &state.config.cors_allowed_origins {
        Some(origins) => AllowOrigin::list(origins.iter().map(|origin| {
            origin
                .parse::<HeaderValue>()
                .expect("CORS_ALLOWED_ORIGINS already validated at startup")
        })),
        None => AllowOrigin::from(Any),
    };
    let cors = CorsLayer::new()
        .allow_origin(allow_origin)
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::IF_MATCH,
        ])
        .max_age(Duration::from_secs(300));

    router
        .merge(protected)
        .layer(TraceLayer::new_for_http())
        .layer(cors)
        .with_state(state)
}
