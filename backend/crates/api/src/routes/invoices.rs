use std::sync::Arc;

use crate::error::ApiError;
use crate::state::AppState;
use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::error::DomainError;
use domain::invoice::{
    Invoice, InvoiceListQuery, InvoiceRepo, InvoiceStatus, NewInvoice, UpdateInvoice,
};
use domain::order::LineItem;
use domain::{AuthContext, ChangeAction, ChangeEvent, LiveChange, Paged, Tenant};
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
    customer_id: Option<String>,
    status: Option<InvoiceStatus>,
    q: Option<String>,
}

impl From<ListParams> for InvoiceListQuery {
    fn from(params: ListParams) -> Self {
        InvoiceListQuery {
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
    status: InvoiceStatus,
}

/// Body for both `POST /api/invoices` and the `POST /api/orders/{id}/invoice`
/// convenience route — the latter supplies `order_id` from the path instead.
#[derive(Debug, Deserialize)]
pub struct NewInvoiceBody {
    currency: Option<String>,
}

async fn repo_for(state: &AppState, auth: &AuthContext) -> Result<Arc<dyn InvoiceRepo>, ApiError> {
    Ok(state.backend.invoice_repo(&auth.tenant_db).await?)
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Paged<Invoice>>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let paged = repo.list(params.into()).await?;
    Ok(Json(paged))
}

#[derive(Debug, Deserialize)]
pub struct CreateBody {
    order_id: String,
    currency: Option<String>,
}

pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<Invoice>), ApiError> {
    let data = NewInvoice {
        order_id: body.order_id,
        currency: body.currency,
    };
    data.validate_domain()?;
    let repo = repo_for(&state, &auth).await?;
    let invoice = repo.create(data, &tenant).await?;
    publish(&state, &auth, ChangeAction::Create, &invoice);
    Ok((StatusCode::CREATED, Json(invoice)))
}

/// `POST /api/orders/{id}/invoice` — sugar over `create` that takes the
/// order id from the path instead of the body.
pub async fn create_from_order(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(order_id): Path<String>,
    Json(body): Json<NewInvoiceBody>,
) -> Result<(StatusCode, Json<Invoice>), ApiError> {
    let data = NewInvoice {
        order_id,
        currency: body.currency,
    };
    data.validate_domain()?;
    let repo = repo_for(&state, &auth).await?;
    let invoice = repo.create(data, &tenant).await?;
    publish(&state, &auth, ChangeAction::Create, &invoice);
    Ok((StatusCode::CREATED, Json(invoice)))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Invoice>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let invoice = repo.get(&id).await?.ok_or(DomainError::NotFound)?;
    Ok(Json(invoice))
}

#[derive(Debug, Deserialize)]
pub struct UpdateBody {
    line_items: Vec<LineItem>,
}

pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
    Json(body): Json<UpdateBody>,
) -> Result<Json<Invoice>, ApiError> {
    let data = UpdateInvoice {
        line_items: body.line_items,
    };
    data.validate_domain()?;
    let repo = repo_for(&state, &auth).await?;
    let invoice = repo.update(&id, data).await?;
    publish(&state, &auth, ChangeAction::Update, &invoice);
    Ok(Json(invoice))
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
        LiveChange::Invoice(Box::new(ChangeEvent {
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
    Path(id): Path<String>,
    Json(body): Json<StatusBody>,
) -> Result<Json<Invoice>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let invoice = repo.set_status(&id, body.status).await?;
    publish(&state, &auth, ChangeAction::Update, &invoice);
    Ok(Json(invoice))
}

fn publish(state: &AppState, auth: &AuthContext, action: ChangeAction, invoice: &Invoice) {
    state.publisher.publish(
        &auth.tenant_db,
        LiveChange::Invoice(Box::new(ChangeEvent {
            action,
            id: invoice.id.clone(),
            data: Some(invoice.clone()),
        })),
    );
}
