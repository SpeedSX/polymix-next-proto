pub mod auth;
pub mod config;
pub mod dev_issuer;
pub mod error;
pub mod jwks;
pub mod routes;
pub mod state;
pub mod ws;

use std::sync::Arc;

use axum::http::{Method, header};
use axum::{Router, middleware, routing::get, routing::post};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

use domain::TenantRepo;

use config::AppConfig;
use dev_issuer::DevIssuer;
use jwks::JwksCache;
use state::AppState;
use surreal_store::{DbConfig, Store, SurrealTenantRepo, TenantProvisioner, migrations};

pub async fn build_state(config: AppConfig) -> anyhow::Result<AppState> {
    let store = Store::connect(&DbConfig {
        url: config.surrealdb_url.clone(),
        user: config.surrealdb_user.clone(),
        pass: config.surrealdb_pass.clone(),
        ns: config.surrealdb_ns.clone(),
    })
    .await?;
    let store = Arc::new(store);
    // Provisioning only runs migrations once, at tenant creation — re-apply
    // them here so a tenant db created before the latest migration was added
    // (e.g. before M2 added order/invoice tables) still gets it, per
    // PLAN.md's "applied per tenant database at provisioning and at
    // startup". `apply_migrations` is idempotent (tracks its own version in
    // `meta:migrations`), so this is a no-op for already-current tenants.
    let tenants = SurrealTenantRepo::new(store.system()).list_all().await?;
    for tenant in tenants {
        let session = store.for_tenant(&tenant.db_name).await?;
        migrations::apply_migrations(&session, &tenant.db_name).await?;
        // Migration 0009 can't zero-pad a backfilled customer number in
        // pure SurrealQL (no `string::format`-style builtin) — this reuses
        // `next_number`'s Rust formatting instead. Idempotent, so safe to
        // run unconditionally alongside migrations on every startup.
        surreal_store::customer_repo::backfill_numbers(&session).await?;
    }
    let provisioner = Arc::new(TenantProvisioner::new(store.clone()));
    let jwks = Arc::new(JwksCache::new(config.auth_jwks_url.clone()));
    let dev_issuer = if config.auth_dev_mode {
        Some(Arc::new(DevIssuer::generate()?))
    } else {
        None
    };

    let hub = Arc::new(ws::hub::Hub::new(store.clone()));

    Ok(AppState {
        config: Arc::new(config),
        store,
        provisioner,
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
