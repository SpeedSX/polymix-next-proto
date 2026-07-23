//! Stateless pricing endpoints (`docs/staff-quoting.md` API) — the calculator.
//!
//! `POST /api/estimate` prices a direct `JobSpec` (tier 2) at each requested
//! quantity; `POST /api/estimate/template` resolves a template selection first
//! (tier 1) and echoes the resolved spec so the UI can open it in the composer.
//! Neither persists anything. Authenticated + tenant-scoped only (relaxed RBAC,
//! same as pricing setup — `quotes:read` gating arrives with B1).

use axum::Extension;
use axum::Json;
use axum::extract::State;
use domain::{AuthContext, Tenant};
use quote_engine::{JobSpec, Selection, price_at, price_job, quote_template, resolve};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::error::ApiError;
use crate::quote_pricing::select_policy;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct EstimateBody {
    job_spec: JobSpec,
    quantities: Vec<u32>,
    #[serde(default)]
    margin_override_bp: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct TemplateEstimateBody {
    template: String,
    selection: Selection,
    #[serde(default)]
    quantities: Option<Vec<u32>>,
    #[serde(default)]
    margin_override_bp: Option<u32>,
}

pub async fn estimate(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Extension(tenant): Extension<Tenant>,
    Json(body): Json<EstimateBody>,
) -> Result<Json<Value>, ApiError> {
    let model = state.backend.price_model(&auth.tenant_db).await?;
    let policy = select_policy(&model, &tenant.default_currency)?;

    let mut results = Vec::with_capacity(body.quantities.len());
    for &qty in &body.quantities {
        let mut spec = body.job_spec.clone();
        spec.quantity = qty;
        let breakdown = price_job(&model, policy, &spec, body.margin_override_bp)?;
        results.push(json!({
            "qty": qty,
            "total_minor": breakdown.total_minor,
            "unit_minor": breakdown.unit_minor,
            "breakdown": breakdown,
        }));
    }

    Ok(Json(json!({
        "currency": policy.currency,
        "policy_name": policy.name,
        "pricelist_version": model.pricelist_version,
        "results": results,
    })))
}

pub async fn estimate_template(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Json(body): Json<TemplateEstimateBody>,
) -> Result<Json<Value>, ApiError> {
    let model = state.backend.price_model(&auth.tenant_db).await?;
    let template = model
        .template_by_slug(&body.template)
        .or_else(|| model.templates.get(&body.template))
        .ok_or_else(|| {
            ApiError::new(
                axum::http::StatusCode::NOT_FOUND,
                "not_found",
                "unknown template",
            )
        })?;

    let quote = quote_template(
        &model,
        template,
        &body.selection,
        body.quantities.as_deref(),
        body.margin_override_bp,
    )?;

    let mut results = Vec::with_capacity(quote.ladder.len());
    for entry in &quote.ladder {
        let breakdown = price_at(
            &model,
            template,
            &body.selection,
            entry.qty,
            body.margin_override_bp,
        )?;
        results.push(json!({
            "qty": entry.qty,
            "total_minor": entry.total_minor,
            "unit_minor": entry.unit_minor,
            "breakdown": breakdown,
        }));
    }

    // Echo the resolved spec at the first quantity so the UI can drop it into
    // the tier-2 composer ("start from template, then tweak").
    let first_qty = quote.ladder.first().map(|e| e.qty).unwrap_or(1);
    let job_spec: JobSpec = resolve(&model, template, &body.selection, first_qty)?;

    Ok(Json(json!({
        "currency": quote.currency,
        "pricelist_version": quote.pricelist_version,
        "results": results,
        "job_spec": job_spec,
    })))
}
