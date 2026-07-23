//! Quote documents (`docs/staff-quoting.md` API).
//!
//! Engine lines are priced here, in the route layer, before the repo persists
//! them: the client submits specs/selections and adjustments, never prices.
//! Authenticated + tenant-scoped only — the `quotes:read/write/override` gates
//! arrive with RBAC (B1); relaxed for now, same as pricing setup.

use std::sync::Arc;

use axum::Extension;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use domain::error::DomainError;
use domain::quote::{
    NewQuote, NewQuoteLine, Quote, QuoteLine, QuoteListQuery, QuoteStatus, QuoteWrite,
};
use domain::{AuthContext, ChangeAction, ChangeEvent, LiveChange, Order, Paged, QuoteRepo, Tenant};
use quote_engine::PriceModel;
use serde::Deserialize;
use serde_json::{Value, json};
use ulid::Ulid;

use crate::error::ApiError;
use crate::quote_pricing::{price_spec, price_template, select_policy};
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
    status: Option<QuoteStatus>,
    q: Option<String>,
}

impl From<ListParams> for QuoteListQuery {
    fn from(params: ListParams) -> Self {
        QuoteListQuery {
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
    status: QuoteStatus,
}

async fn repo_for(state: &AppState, auth: &AuthContext) -> Result<Arc<dyn QuoteRepo>, ApiError> {
    Ok(state.backend.quote_repo(&auth.tenant_db).await?)
}

fn line_id_or_new(existing: Option<String>) -> String {
    existing.unwrap_or_else(|| Ulid::new().to_string())
}

/// Price the client's submitted lines into stored [`QuoteLine`]s. Returns the
/// lines plus the `pricelist_version` to stamp (`Some` iff any line is
/// engine-priced).
fn price_lines(
    model: &PriceModel,
    currency: &str,
    lines: Vec<NewQuoteLine>,
) -> Result<(Vec<QuoteLine>, Option<i64>), ApiError> {
    let mut out = Vec::with_capacity(lines.len());
    let mut has_engine_line = false;
    for line in lines {
        match line {
            NewQuoteLine::Manual {
                line_id,
                description,
                qty,
                unit_minor,
            } => out.push(QuoteLine::Manual {
                line_id: line_id_or_new(line_id),
                description,
                qty,
                unit_minor,
            }),
            NewQuoteLine::Spec {
                line_id,
                job_spec,
                description,
                qty,
                adjustment,
            } => {
                has_engine_line = true;
                let policy = select_policy(model, currency)?;
                let pricing = price_spec(model, policy, &job_spec, qty, adjustment.as_ref())?;
                out.push(QuoteLine::Spec {
                    line_id: line_id_or_new(line_id),
                    job_spec,
                    description,
                    qty,
                    pricing,
                });
            }
            NewQuoteLine::Template {
                line_id,
                template,
                selection,
                qty,
                adjustment,
            } => {
                has_engine_line = true;
                let tmpl = model
                    .template_by_slug(&template)
                    .or_else(|| model.templates.get(&template))
                    .ok_or_else(|| {
                        ApiError::new(StatusCode::NOT_FOUND, "not_found", "unknown template")
                    })?;
                let pricing = price_template(model, tmpl, &selection, qty, adjustment.as_ref())?;
                out.push(QuoteLine::Template {
                    line_id: line_id_or_new(line_id),
                    template: tmpl.id.clone(),
                    selection,
                    qty,
                    pricing,
                });
            }
        }
    }
    let version = has_engine_line.then_some(model.pricelist_version);
    Ok((out, version))
}

/// Re-price existing engine lines against the current model (reprice action),
/// preserving line ids and adjustments. Manual lines pass through unchanged.
fn reprice_lines(
    model: &PriceModel,
    currency: &str,
    lines: &[QuoteLine],
) -> Result<(Vec<QuoteLine>, Option<i64>), ApiError> {
    let mut out = Vec::with_capacity(lines.len());
    let mut has_engine_line = false;
    for line in lines {
        match line {
            QuoteLine::Manual { .. } => out.push(line.clone()),
            QuoteLine::Spec {
                line_id,
                job_spec,
                description,
                qty,
                pricing,
            } => {
                has_engine_line = true;
                let policy = select_policy(model, currency)?;
                let repriced =
                    price_spec(model, policy, job_spec, *qty, pricing.adjustment.as_ref())?;
                out.push(QuoteLine::Spec {
                    line_id: line_id.clone(),
                    job_spec: job_spec.clone(),
                    description: description.clone(),
                    qty: *qty,
                    pricing: repriced,
                });
            }
            QuoteLine::Template {
                line_id,
                template,
                selection,
                qty,
                pricing,
            } => {
                has_engine_line = true;
                let tmpl = model
                    .template_by_slug(template)
                    .or_else(|| model.templates.get(template))
                    .ok_or_else(|| {
                        ApiError::new(StatusCode::NOT_FOUND, "not_found", "unknown template")
                    })?;
                let repriced =
                    price_template(model, tmpl, selection, *qty, pricing.adjustment.as_ref())?;
                out.push(QuoteLine::Template {
                    line_id: line_id.clone(),
                    template: template.clone(),
                    selection: selection.clone(),
                    qty: *qty,
                    pricing: repriced,
                });
            }
        }
    }
    let version = has_engine_line.then_some(model.pricelist_version);
    Ok((out, version))
}

async fn build_write(
    state: &AppState,
    auth: &AuthContext,
    tenant: &Tenant,
    mut body: NewQuote,
    created_by: String,
) -> Result<QuoteWrite, ApiError> {
    body.resolve_currency(&tenant.default_currency);
    body.validate_domain()?;
    let currency = body
        .currency
        .clone()
        .expect("resolve_currency always sets a value");
    let model = state.backend.price_model(&auth.tenant_db).await?;
    let (lines, pricelist_version) = price_lines(&model, &currency, body.lines)?;
    Ok(QuoteWrite {
        customer_id: body.customer_id,
        prospect: body.prospect,
        currency,
        valid_until: body.valid_until,
        notes: body.notes,
        lines,
        pricelist_version,
        created_by,
    })
}

pub async fn list(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(params): Query<ListParams>,
) -> Result<Json<Paged<Quote>>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    Ok(Json(repo.list(params.into()).await?))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Json(body): Json<NewQuote>,
) -> Result<(StatusCode, Json<Quote>), ApiError> {
    let write = build_write(&state, &auth, &tenant, body, auth.user_id.clone()).await?;
    let repo = repo_for(&state, &auth).await?;
    let quote = repo.create(write, &tenant).await?;
    publish(&state, &auth, ChangeAction::Create, &quote);
    Ok((StatusCode::CREATED, Json(quote)))
}

pub async fn get(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Quote>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let quote = repo.get(&id).await?.ok_or(DomainError::NotFound)?;
    Ok(Json(quote))
}

pub async fn update(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
    Json(body): Json<NewQuote>,
) -> Result<Json<Quote>, ApiError> {
    // `created_by` is preserved by the repo; the value here is ignored on update.
    let write = build_write(&state, &auth, &tenant, body, auth.user_id.clone()).await?;
    let repo = repo_for(&state, &auth).await?;
    let quote = repo.update(&id, write).await?;
    publish(&state, &auth, ChangeAction::Update, &quote);
    Ok(Json(quote))
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
        LiveChange::Quote(Box::new(ChangeEvent {
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
) -> Result<Json<Quote>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let quote = repo.set_status(&id, body.status).await?;
    publish(&state, &auth, ChangeAction::Update, &quote);
    Ok(Json(quote))
}

pub async fn reprice(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path(id): Path<String>,
) -> Result<Json<Value>, ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let existing = repo.get(&id).await?.ok_or(DomainError::NotFound)?;
    if !domain::quote::can_edit(existing.status) {
        return Err(DomainError::Conflict(domain::error::ConflictReason::QuoteNotDraft).into());
    }
    let model = state.backend.price_model(&auth.tenant_db).await?;
    let (lines, pricelist_version) = reprice_lines(&model, &existing.currency, &existing.lines)?;

    // Which lines moved — the "price changed on reprice" flag (design 14a).
    let changed: Vec<String> = existing
        .lines
        .iter()
        .filter_map(|old| {
            let new = lines.iter().find(|l| l.line_id() == old.line_id())?;
            (new.total_minor() != old.total_minor()).then(|| old.line_id().to_string())
        })
        .collect();

    let write = QuoteWrite {
        customer_id: existing.customer_id,
        prospect: existing.prospect,
        currency: existing.currency,
        valid_until: existing.valid_until,
        notes: existing.notes,
        lines,
        pricelist_version,
        created_by: existing.created_by,
    };
    let quote = repo.update(&id, write).await?;
    publish(&state, &auth, ChangeAction::Update, &quote);
    Ok(Json(json!({ "quote": quote, "changed_line_ids": changed })))
}

pub async fn clone(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Quote>), ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let quote = repo.clone_quote(&id, &tenant, &auth.user_id).await?;
    publish(&state, &auth, ChangeAction::Create, &quote);
    Ok((StatusCode::CREATED, Json(quote)))
}

pub async fn convert_to_order(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Path(id): Path<String>,
) -> Result<(StatusCode, Json<Order>), ApiError> {
    let repo = repo_for(&state, &auth).await?;
    let order = repo
        .convert_to_order(&id, &tenant, chrono::Utc::now())
        .await?;
    // The new order and the quote's freshly-set order link both go out live.
    state.publisher.publish(
        &auth.tenant_db,
        LiveChange::Order(Box::new(ChangeEvent {
            action: ChangeAction::Create,
            id: order.id.clone(),
            data: Some(order.clone()),
        })),
    );
    if let Ok(Some(quote)) = repo.get(&id).await {
        publish(&state, &auth, ChangeAction::Update, &quote);
    }
    Ok((StatusCode::CREATED, Json(order)))
}

fn publish(state: &AppState, auth: &AuthContext, action: ChangeAction, quote: &Quote) {
    state.publisher.publish(
        &auth.tenant_db,
        LiveChange::Quote(Box::new(ChangeEvent {
            action,
            id: quote.id.clone(),
            data: Some(quote.clone()),
        })),
    );
}
