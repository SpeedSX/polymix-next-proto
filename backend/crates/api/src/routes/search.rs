use axum::Extension;
use axum::Json;
use axum::extract::{Query, State};
use domain::{AuthContext, SearchHit, SearchResults};
use serde::Deserialize;

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

    // Fan out across entity repositories concurrently rather than adding up
    // each backend setup + query round-trip sequentially.
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
    let repo = state.backend.customer_repo(tenant_db).await?;
    Ok(repo.search(q, HITS_PER_ENTITY).await?)
}

async fn search_orders(
    state: &AppState,
    tenant_db: &str,
    q: &str,
) -> Result<Vec<SearchHit>, ApiError> {
    let repo = state.backend.order_repo(tenant_db).await?;
    Ok(repo.search(q, HITS_PER_ENTITY).await?)
}

async fn search_invoices(
    state: &AppState,
    tenant_db: &str,
    q: &str,
) -> Result<Vec<SearchHit>, ApiError> {
    let repo = state.backend.invoice_repo(tenant_db).await?;
    Ok(repo.search(q, HITS_PER_ENTITY).await?)
}
