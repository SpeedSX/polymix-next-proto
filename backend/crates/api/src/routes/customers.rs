use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::customer::{Customer, ListQuery, NewCustomer, Paged};
use domain::error::DomainError;
use domain::{AuthContext, CustomerRepo};
use serde::Deserialize;
use serde_json::{Value, json};
use surreal_store::SurrealCustomerRepo;

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
}

impl From<ListParams> for ListQuery {
    fn from(params: ListParams) -> Self {
        ListQuery {
            page: params.page.max(1),
            limit: params.limit.clamp(1, 100),
            sort: params.sort,
        }
    }
}

async fn repo_for(state: &AppState, auth: &AuthContext) -> Result<SurrealCustomerRepo, ApiError> {
    let session = state
        .store
        .for_tenant(&auth.tenant_db)
        .await
        .map_err(|err| {
            tracing::error!(error = %err, "failed to open tenant session");
            ApiError::internal("internal server error")
        })?;
    Ok(SurrealCustomerRepo::new(session))
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Paged<Customer>>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let paged = repo.list(params.into()).await?;
    Ok(Json(paged))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<NewCustomer>,
) -> Result<(StatusCode, Json<Customer>), ApiError> {
    body.validate_domain()?;
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.create(body).await?;
    Ok((StatusCode::CREATED, Json(customer)))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Customer>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.get(&id).await?.ok_or(DomainError::NotFound)?;
    Ok(Json(customer))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<NewCustomer>,
) -> Result<Json<Customer>, ApiError> {
    body.validate_domain()?;
    let repo = repo_for(&state, &auth).await?;
    let customer = repo.update(&id, body).await?;
    Ok(Json(customer))
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
