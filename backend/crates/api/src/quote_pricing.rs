//! Engine pricing for the quote/estimate routes (`docs/staff-quoting.md`).
//!
//! The route layer — not the repo — owns pricing, because it holds the
//! in-memory [`PriceModel`] snapshot. These helpers turn a client-submitted
//! spec + optional [`Adjustment`] into the stored [`EnginePricing`] audit
//! record, and pick the pricing policy for a currency.

use axum::http::StatusCode;
use domain::quote::{Adjustment, AdjustmentKind, EnginePricing};
use quote_engine::{
    JobSpec, PriceModel, PricingPolicy, ProductTemplate, Selection, price_at, price_job,
};

use crate::error::ApiError;

/// The pricing policy for `currency`: the first policy denominated in it, else
/// the first policy of any currency (v1 tenants run a single policy). A tenant
/// with no policy configured can't price anything — a 422 the UI can surface.
pub fn select_policy<'a>(
    model: &'a PriceModel,
    currency: &str,
) -> Result<&'a PricingPolicy, ApiError> {
    model
        .pricing_policies
        .values()
        .find(|p| p.currency == currency)
        .or_else(|| model.pricing_policies.values().next())
        .ok_or_else(|| {
            ApiError::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                "no_pricing_policy",
                "no pricing policy is configured for this tenant",
            )
        })
}

/// Price one spec at `qty` into an [`EnginePricing`]. `engine_total_minor` is
/// always the policy-band price (no adjustment); `final_total_minor` applies
/// the adjustment: a `MarginOverride` is re-priced through the engine (spec
/// delta 3), `Discount`/`PriceOverride` are applied to the engine result. The
/// stored breakdown is always the band breakdown — the audit "engine price".
pub fn price_spec(
    model: &PriceModel,
    policy: &PricingPolicy,
    job_spec: &JobSpec,
    qty: u32,
    adjustment: Option<&Adjustment>,
) -> Result<EnginePricing, ApiError> {
    let mut spec = job_spec.clone();
    spec.quantity = qty;
    let breakdown = price_job(model, policy, &spec, None)?;
    let engine_total_minor = breakdown.total_minor;

    let final_total_minor = match adjustment {
        Some(Adjustment {
            kind: AdjustmentKind::MarginOverride { multiplier_bp },
            ..
        }) => price_job(model, policy, &spec, Some(*multiplier_bp))?.total_minor,
        Some(adj) => adj.apply(engine_total_minor),
        None => engine_total_minor,
    };

    Ok(EnginePricing {
        breakdown,
        engine_total_minor,
        adjustment: adjustment.cloned(),
        final_total_minor,
    })
}

/// Tier-1 analogue of [`price_spec`]: resolve a template selection at `qty` and
/// price it, applying the same adjustment semantics. The policy comes from the
/// template (`price_at`), not the currency.
pub fn price_template(
    model: &PriceModel,
    template: &ProductTemplate,
    selection: &Selection,
    qty: u32,
    adjustment: Option<&Adjustment>,
) -> Result<EnginePricing, ApiError> {
    let breakdown = price_at(model, template, selection, qty, None)?;
    let engine_total_minor = breakdown.total_minor;

    let final_total_minor = match adjustment {
        Some(Adjustment {
            kind: AdjustmentKind::MarginOverride { multiplier_bp },
            ..
        }) => price_at(model, template, selection, qty, Some(*multiplier_bp))?.total_minor,
        Some(adj) => adj.apply(engine_total_minor),
        None => engine_total_minor,
    };

    Ok(EnginePricing {
        breakdown,
        engine_total_minor,
        adjustment: adjustment.cloned(),
        final_total_minor,
    })
}
