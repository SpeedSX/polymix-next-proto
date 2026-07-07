use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use domain::{AuthContext, CustomerRepo, InvoiceRepo, OrderRepo, SearchHit, SearchResults};
use serde::Deserialize;
use surreal_store::{SurrealCustomerRepo, SurrealInvoiceRepo, SurrealOrderRepo};

use crate::error::ApiError;
use crate::state::AppState;

/// Max hits per entity in the global omnibox response, per PLAN.md.
const HITS_PER_ENTITY: u32 = 5;

#[derive(Debug, Default, Deserialize)]
pub struct SearchParams {
    #[serde(default)]
    q: Option<String>,
}

pub async fn search(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<SearchParams>,
) -> Result<Json<SearchResults>, ApiError> {
    let q = params.q.as_deref().unwrap_or("").trim();
    if q.is_empty() {
        return Ok(Json(SearchResults::default()));
    }

    // Fan out across entities concurrently rather than adding up each one's
    // session-open + query round-trip sequentially. Each gets its own fresh
    // `for_tenant()` session rather than `.clone()`-ing one already returned
    // by `for_tenant()` — see docs/adr/0002 for why that hangs.
    let tenant_db = auth.tenant_db.as_str();
    let (customers, orders, invoices) = tokio::try_join!(
        search_customers(&state, tenant_db, q),
        search_orders(&state, tenant_db, q),
        search_invoices(&state, tenant_db, q),
    )?;

    Ok(Json(SearchResults {
        customers,
        orders,
        invoices,
    }))
}

async fn search_customers(
    state: &AppState,
    tenant_db: &str,
    q: &str,
) -> Result<Vec<SearchHit>, ApiError> {
    let session = state.store.for_tenant(tenant_db).await.map_err(|err| {
        tracing::error!(error = %err, "failed to open tenant session");
        ApiError::internal("internal server error")
    })?;
    Ok(SurrealCustomerRepo::new(session)
        .search(q, HITS_PER_ENTITY)
        .await?)
}

async fn search_orders(
    state: &AppState,
    tenant_db: &str,
    q: &str,
) -> Result<Vec<SearchHit>, ApiError> {
    let session = state.store.for_tenant(tenant_db).await.map_err(|err| {
        tracing::error!(error = %err, "failed to open tenant session");
        ApiError::internal("internal server error")
    })?;
    Ok(SurrealOrderRepo::new(session)
        .search(q, HITS_PER_ENTITY)
        .await?)
}

async fn search_invoices(
    state: &AppState,
    tenant_db: &str,
    q: &str,
) -> Result<Vec<SearchHit>, ApiError> {
    let session = state.store.for_tenant(tenant_db).await.map_err(|err| {
        tracing::error!(error = %err, "failed to open tenant session");
        ApiError::internal("internal server error")
    })?;
    Ok(SurrealInvoiceRepo::new(session)
        .search(q, HITS_PER_ENTITY)
        .await?)
}
