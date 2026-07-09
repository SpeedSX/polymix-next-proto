use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::error::DomainError;
use domain::order::{NewOrder, Order, OrderListQuery, OrderStatus, validate_line_item_currencies};
use domain::{AuthContext, OrderRepo, Paged, Tenant};
use serde::Deserialize;
use serde_json::{Value, json};
use surreal_store::SurrealOrderRepo;

use crate::error::ApiError;
use crate::state::AppState;

fn default_page() -> u32 {
    1
}
fn default_limit() -> u32 {
    25
}
fn default_sort() -> String {
    "-created_at".to_string()
}

#[derive(Debug, Deserialize)]
pub struct ListParams {
    #[serde(default = "default_page")]
    page: u32,
    #[serde(default = "default_limit")]
    limit: u32,
    #[serde(default = "default_sort")]
    sort: String,
    customer_id: Option<String>,
    status: Option<OrderStatus>,
    q: Option<String>,
}

impl From<ListParams> for OrderListQuery {
    fn from(params: ListParams) -> Self {
        OrderListQuery {
            page: params.page.max(1),
            limit: params.limit.clamp(1, 100),
            sort: params.sort,
            customer_id: params.customer_id,
            status: params.status,
            q: params.q,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StatusBody {
    status: OrderStatus,
}

async fn repo_for(state: &AppState, auth: &AuthContext) -> Result<SurrealOrderRepo, ApiError> {
    let session = state
        .store
        .for_tenant(&auth.tenant_db)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "failed to open tenant session");
            ApiError::internal("internal server error")
        })?;
    Ok(SurrealOrderRepo::new(session))
}

/// Resolves the order's currency default and checks every line item is
/// denominated in it — shared by `create` and `update`, both of which
/// accept a full `NewOrder` body.
fn prepare(body: &mut NewOrder, tenant: &Tenant) -> Result<(), ApiError> {
    body.resolve_currency(&tenant.default_currency);
    body.validate_domain()?;
    let currency = body
        .currency
        .as_deref()
        .expect("resolve_currency always sets a value");
    validate_line_item_currencies(&body.line_items, currency)?;
    Ok(())
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Paged<Order>>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let paged = repo.list(params.into()).await?;
    Ok(Json(paged))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Json(mut body): Json<NewOrder>,
) -> Result<(StatusCode, Json<Order>), ApiError> {
    prepare(&mut body, &tenant)?;
    let repo = repo_for(&state, &auth).await?;
    let order = repo.create(body, &tenant).await?;
    Ok((StatusCode::CREATED, Json(order)))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Order>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let order = repo.get(&id).await?.ok_or(DomainError::NotFound)?;
    Ok(Json(order))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
    Json(mut body): Json<NewOrder>,
) -> Result<Json<Order>, ApiError> {
    prepare(&mut body, &tenant)?;
    let repo = repo_for(&state, &auth).await?;
    let order = repo.update(&id, body).await?;
    Ok(Json(order))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    repo.delete(&id).await?;
    Ok(Json(json!({})))
}

pub async fn set_status(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Order>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let order = repo.set_status(&id, body.status).await?;
    Ok(Json(order))
}
