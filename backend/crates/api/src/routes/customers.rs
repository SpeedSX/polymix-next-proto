use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use domain::customer::{Customer, CustomerStatus, ListQuery, NewCustomer, Paged};
use domain::error::DomainError;
use domain::{AuthContext, ChangeAction, ChangeEvent, CustomerRepo, LiveChange, Tenant};
use serde::Deserialize;
use serde_json::{Value, json};

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
    q: Option<String>,
    status: Option<CustomerStatus>,
    tag: Option<String>,
}

impl From<ListParams> for ListQuery {
    fn from(params: ListParams) -> Self {
        ListQuery {
            page: params.page.max(1),
            limit: params.limit.clamp(1, 100),
            sort: params.sort,
            q: params.q,
            status: params.status,
            tag: params.tag,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct StatusBody {
    status: CustomerStatus,
}

async fn repo_for(state: &AppState, auth: &AuthContext) -> Result<Arc<dyn CustomerRepo>, ApiError> {
    Ok(state.backend.customer_repo(&auth.tenant_db).await?)
}

/// Normalizes tags, resolves the tenant's default currency, and runs domain
/// validation — shared by `create` and `update`, both of which accept a
/// full `NewCustomer` body.
fn prepare(body: &mut NewCustomer, tenant: &Tenant) -> Result<(), ApiError> {
    body.normalize();
    body.resolve_default_currency(&tenant.default_currency);
    body.validate_domain()?;
    Ok(())
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Query(params): Query<ListParams>,
) -> Result<Json<Paged<Customer>>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let paged = repo.list(params.into(), &tenant).await?;
    Ok(Json(paged))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Json(mut body): Json<NewCustomer>,
) -> Result<(StatusCode, Json<Customer>), ApiError> {
    prepare(&mut body, &tenant)?;
    body.validate_creation_status()?;
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.create(body, &tenant).await?;
    publish(&state, &auth, ChangeAction::Create, &customer);
    Ok((StatusCode::CREATED, Json(customer)))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
) -> Result<Json<Customer>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.get(&id, &tenant).await?.ok_or(DomainError::NotFound)?;
    Ok(Json(customer))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(mut body): Json<NewCustomer>,
) -> Result<Json<Customer>, ApiError> {
    prepare(&mut body, &tenant)?;
    // `If-Match` carries the customer `version` the client last saw — an
    // optimistic-concurrency token. Absent means an unconditional write; a
    // present-but-unparseable value is a client bug, not a silent full write.
    let expected_version = match headers.get(header::IF_MATCH) {
        Some(value) => Some(
            value
                .to_str()
                .ok()
                .and_then(|raw| raw.trim_matches('"').parse::<i64>().ok())
                .ok_or_else(|| {
                    ApiError::new(
                        StatusCode::BAD_REQUEST,
                        "invalid_if_match",
                        "If-Match must be an integer customer version",
                    )
                })?,
        ),
        None => None,
    };
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.update(&id, body, expected_version, &tenant).await?;
    publish(&state, &auth, ChangeAction::Update, &customer);
    Ok(Json(customer))
}

pub async fn delete(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    repo.delete(&id).await?;
    state.publisher.publish(
        &auth.tenant_db,
        LiveChange::Customer(Box::new(ChangeEvent {
            action: ChangeAction::Delete,
            id,
            data: None,
        })),
    );
    Ok(Json(json!({})))
}

pub async fn set_status(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Customer>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.set_status(&id, body.status, &tenant).await?;
    publish(&state, &auth, ChangeAction::Update, &customer);
    Ok(Json(customer))
}

fn publish(state: &AppState, auth: &AuthContext, action: ChangeAction, customer: &Customer) {
    state.publisher.publish(
        &auth.tenant_db,
        LiveChange::Customer(Box::new(ChangeEvent {
            action,
            id: customer.id.clone(),
            data: Some(customer.clone()),
        })),
    );
}
