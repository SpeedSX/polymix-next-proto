//! §6 — pricing: a `JobSpec` becomes a cost [`Breakdown`], and a template +
//! selection becomes a price [`Quote`] ladder.
//!
//! Margin override (staff-quoting delta 3) replaces the policy band multiplier;
//! [`Breakdown`] is the normative back-office breakdown schema (delta 4). All
//! arithmetic is integer µ-units.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{EngineError, PriceError, ResolveError};
use crate::model::{
    Colors, CustomQuantity, Machine, PriceModel, PricingPolicy, ProductTemplate, Technology,
    UnitBasis,
};
use crate::money::{BLEED_MM, MICRO_PER_MINOR, ceil_div, round_half_up};
use crate::resolve::{Component, JobSpec, OperationInstance, Selection, resolve};
use crate::rules;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Breakdown {
    pub components: Vec<ComponentCost>,
    pub operations: Vec<OperationCost>,
    pub cost_micro: i64,
    pub total_minor: i64,
    pub unit_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentCost {
    pub role: String,
    /// `None` for an unprinted (`0/0`) component — no machine involved.
    pub machine_id: Option<String>,
    pub sheets: u32,
    pub cost_micro: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationCost {
    pub operation: String,
    pub cost_micro: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LadderEntry {
    pub qty: u32,
    pub total_minor: i64,
    pub unit_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quote {
    pub currency: String,
    pub pricelist_version: i64,
    pub ladder: Vec<LadderEntry>,
}

/// §6.1 imposition: how many trimmed items fit on a press sheet, trying both
/// orientations. `0` means the item is larger than the sheet.
pub fn ups(sheet: [u32; 2], trim: [u32; 2]) -> u32 {
    let fp = [trim[0] + 2 * BLEED_MM, trim[1] + 2 * BLEED_MM];
    let straight = (sheet[0] / fp[0]) * (sheet[1] / fp[1]);
    let rotated = (sheet[0] / fp[1]) * (sheet[1] / fp[0]);
    straight.max(rotated)
}

/// Price a fully-resolved spec into a cost breakdown (§6.2–§6.4). `qty` comes
/// from `job.quantity`.
pub fn price_job(
    model: &PriceModel,
    policy: &PricingPolicy,
    job: &JobSpec,
    margin_override: Option<u32>,
) -> Result<Breakdown, EngineError> {
    let format = model
        .formats
        .get(&job.format)
        .ok_or_else(|| ResolveError::DanglingReference(job.format.clone()))?;
    let trim = format.trim_mm;
    let qty = job.quantity as i64;

    let components: Vec<ComponentCost> = job
        .components
        .iter()
        .map(|c| cost_component(model, trim, job.technology_allow.as_deref(), qty, c))
        .collect::<Result<_, _>>()?;

    let total_sheets: i64 = components.iter().map(|c| c.sheets as i64).sum();

    let operations: Vec<OperationCost> = job
        .operations
        .iter()
        .map(|op| cost_operation(model, trim, qty, total_sheets, op))
        .collect::<Result<_, _>>()?;

    let cost_micro: i64 = components.iter().map(|c| c.cost_micro).sum::<i64>()
        + operations.iter().map(|o| o.cost_micro).sum::<i64>();

    let (total_minor, unit_minor) = apply_margin(policy, cost_micro, job.quantity, margin_override);

    Ok(Breakdown {
        components,
        operations,
        cost_micro,
        total_minor,
        unit_minor,
    })
}

fn cost_component(
    model: &PriceModel,
    trim: [u32; 2],
    technology_allow: Option<&[Technology]>,
    qty: i64,
    comp: &Component,
) -> Result<ComponentCost, EngineError> {
    let material = model
        .materials
        .get(&comp.material)
        .ok_or_else(|| ResolveError::DanglingReference(comp.material.clone()))?;
    let sheet_size = material
        .pricing
        .sheet_size_mm()
        .ok_or_else(|| ResolveError::DanglingReference(comp.material.clone()))?;
    let price_micro = material.pricing.price_micro();

    let ups_val = ups(sheet_size, trim);
    if ups_val == 0 {
        return Err(PriceError::ItemLargerThanSheet(comp.role.clone()).into());
    }
    let leaves = ceil_div(comp.pages as i64, 2);
    let raw_sheets = ceil_div(qty * leaves, ups_val as i64);

    // Unprinted component: no machine, no waste (§6.2).
    if comp.colors.is_unprinted() {
        return Ok(ComponentCost {
            role: comp.role.clone(),
            machine_id: None,
            sheets: raw_sheets as u32,
            cost_micro: raw_sheets * price_micro,
        });
    }

    let printable = material.printable;
    let mut best: Option<(&str, i64, i64)> = None; // (machine id, sheets, cost)
    for machine in model.machines.values() {
        if let Some(pin) = &comp.machine
            && &machine.id != pin
        {
            continue;
        }
        if !is_capable(
            machine,
            technology_allow,
            comp.colors,
            printable,
            sheet_size,
            trim,
        ) {
            continue;
        }
        let (sheets, cost) = machine_cost(machine, raw_sheets, price_micro, comp.colors);
        // Strict `<` keeps the earliest machine (BTreeMap iterates by id) on a
        // tie — the §6.2 determinism rule.
        if best.is_none_or(|(_, _, best_cost)| cost < best_cost) {
            best = Some((&machine.id, sheets, cost));
        }
    }

    let (machine_id, sheets, cost) =
        best.ok_or_else(|| PriceError::NoCapableMachine(comp.role.clone()))?;
    Ok(ComponentCost {
        role: comp.role.clone(),
        machine_id: Some(machine_id.to_string()),
        sheets: sheets as u32,
        cost_micro: cost,
    })
}

fn is_capable(
    machine: &Machine,
    technology_allow: Option<&[Technology]>,
    colors: Colors,
    printable: Option<crate::model::Printable>,
    sheet_size: [u32; 2],
    trim: [u32; 2],
) -> bool {
    let tech_ok = technology_allow.is_none_or(|allow| allow.contains(&machine.technology));
    let duplex_ok = colors.back == 0 || machine.duplex;
    let grammage_ok = printable.is_some_and(|p| p.grammage_gsm <= machine.max_grammage_gsm);
    let sheet_ok = sheet_size == machine.sheet_size_mm;
    let fits = ups(machine.sheet_size_mm, trim) >= 1;
    tech_ok && duplex_ok && grammage_ok && sheet_ok && fits
}

/// Returns `(sheets, cost_micro)` for one capable machine (§6.2).
fn machine_cost(
    machine: &Machine,
    raw_sheets: i64,
    price_micro: i64,
    colors: Colors,
) -> (i64, i64) {
    let sheets = ceil_div(raw_sheets * (100 + machine.waste_percent as i64), 100)
        + machine.waste_fixed_sheets as i64;
    let cost = match machine.technology {
        Technology::Digital => {
            let clicks = side_price(machine, colors.front) + side_price(machine, colors.back);
            machine.setup_micro + sheets * price_micro + sheets * clicks
        }
        Technology::Offset => {
            let plates = (colors.front + colors.back) as i64;
            machine.setup_micro
                + plates * machine.plate_price_micro
                + sheets * (price_micro + machine.run_price_micro)
        }
    };
    (sheets, cost)
}

fn side_price(machine: &Machine, inks: u8) -> i64 {
    match inks {
        0 => 0,
        1 => machine.click_mono_micro,
        _ => machine.click_color_micro,
    }
}

fn cost_operation(
    model: &PriceModel,
    trim: [u32; 2],
    qty: i64,
    total_sheets: i64,
    op: &OperationInstance,
) -> Result<OperationCost, EngineError> {
    let operation = model
        .operations
        .get(&op.operation)
        .ok_or_else(|| ResolveError::DanglingReference(op.operation.clone()))?;

    let mut unit = operation.unit_price_micro;
    if let Some(material_value) = op.params.get("material") {
        let material_id = material_value
            .as_str()
            .ok_or_else(|| ResolveError::DanglingReference(op.operation.clone()))?;
        let material = model
            .materials
            .get(material_id)
            .ok_or_else(|| ResolveError::DanglingReference(material_id.to_string()))?;
        if material.pricing.basis() != operation.unit_basis {
            return Err(PriceError::MaterialBasisMismatch(op.operation.clone()).into());
        }
        unit += material.pricing.price_micro();
    }

    let multiplier = reserved_u32(op, "units_multiplier")?.unwrap_or(1);
    if multiplier < 1 {
        return Err(PriceError::InvalidReservedParam(op.operation.clone()).into());
    }
    let m = multiplier as i64;
    let setup = operation.setup_micro;

    let cost = match operation.unit_basis {
        UnitBasis::PerItem => setup + m * qty * unit,
        UnitBasis::PerSheet => setup + m * total_sheets * unit,
        UnitBasis::PerCm => {
            let edge = match reserved_u32(op, "edge_mm")? {
                Some(e) => e as i64,
                None => trim[1] as i64,
            };
            setup + round_half_up(m * qty * edge * unit, 10)
        }
        UnitBasis::PerM2 => {
            let area = trim[0] as i64 * trim[1] as i64;
            setup + round_half_up(m * qty * area * unit, 1_000_000)
        }
    };

    Ok(OperationCost {
        operation: op.operation.clone(),
        cost_micro: cost,
    })
}

/// Read a reserved `u32` param. `None` = absent; `Err(E205)` = present but not a
/// positive integer (`units_multiplier < 1`/non-integer, `edge_mm = 0`).
fn reserved_u32(op: &OperationInstance, key: &str) -> Result<Option<u32>, PriceError> {
    match op.params.get(key) {
        None => Ok(None),
        Some(Value::Number(n)) => n
            .as_u64()
            .filter(|v| *v >= 1)
            .and_then(|v| u32::try_from(v).ok())
            .map(Some)
            .ok_or_else(|| PriceError::InvalidReservedParam(op.operation.clone())),
        Some(_) => Err(PriceError::InvalidReservedParam(op.operation.clone())),
    }
}

/// §6.4 total, margin, rounding. Returns `(total_minor, unit_minor)`.
fn apply_margin(
    policy: &PricingPolicy,
    cost_micro: i64,
    qty: u32,
    margin_override: Option<u32>,
) -> (i64, i64) {
    let band_multiplier = policy.band_for(qty).map(|b| b.multiplier_bp).unwrap_or(0);
    let multiplier_bp = margin_override.unwrap_or(band_multiplier) as i64;

    let price_micro = round_half_up(cost_micro * multiplier_bp, 10_000);
    let mut total_minor = ceil_div(price_micro, MICRO_PER_MINOR);
    let step = policy.rounding.step_minor;
    total_minor = ceil_div(total_minor, step) * step;
    total_minor = total_minor.max(policy.min_price_minor);
    let unit_minor = round_half_up(total_minor, qty as i64);
    (total_minor, unit_minor)
}

/// Full quote ladder for a template + selection (§6.4, §7). Evaluates
/// compatibility rules against every ladder quantity and aborts with
/// `RuleViolation` if any rule is violated.
pub fn quote_template(
    model: &PriceModel,
    template: &ProductTemplate,
    selection: &Selection,
    requested_quantities: Option<&[u32]>,
    margin_override: Option<u32>,
) -> Result<Quote, EngineError> {
    let policy = model
        .pricing_policies
        .get(&template.pricing_policy)
        .ok_or_else(|| ResolveError::DanglingReference(template.pricing_policy.clone()))?;

    let quantities = ladder_quantities(template, requested_quantities)?;
    let specs = quantities
        .iter()
        .map(|&qty| resolve(model, template, selection, qty))
        .collect::<Result<Vec<_>, _>>()?;

    let rules = model.rules_for(&template.id);
    let mut all_violations = Vec::new();
    for spec in &specs {
        for violation in rules::violations(rules, spec) {
            if !all_violations
                .iter()
                .any(|v: &crate::error::RuleViolation| v.rule_id == violation.rule_id)
            {
                all_violations.push(violation);
            }
        }
    }
    if !all_violations.is_empty() {
        return Err(EngineError::RuleViolation(all_violations));
    }

    let ladder = specs
        .iter()
        .map(|spec| {
            let bd = price_job(model, policy, spec, margin_override)?;
            Ok(LadderEntry {
                qty: spec.quantity,
                total_minor: bd.total_minor,
                unit_minor: bd.unit_minor,
            })
        })
        .collect::<Result<_, EngineError>>()?;

    Ok(Quote {
        currency: policy.currency.clone(),
        pricelist_version: model.pricelist_version,
        ladder,
    })
}

/// Single-quantity price with full breakdown — the back-office / golden-test
/// entry point. Does not evaluate compatibility rules.
pub fn price_at(
    model: &PriceModel,
    template: &ProductTemplate,
    selection: &Selection,
    qty: u32,
    margin_override: Option<u32>,
) -> Result<Breakdown, EngineError> {
    let policy = model
        .pricing_policies
        .get(&template.pricing_policy)
        .ok_or_else(|| ResolveError::DanglingReference(template.pricing_policy.clone()))?;
    let spec = resolve(model, template, selection, qty)?;
    price_job(model, policy, &spec, margin_override)
}

/// Staff direct-spec ladder (tier 2): price one spec at each requested quantity.
/// No template, no ladder defaults, no rules.
pub fn quote_spec(
    model: &PriceModel,
    policy: &PricingPolicy,
    spec: &JobSpec,
    quantities: &[u32],
    margin_override: Option<u32>,
) -> Result<Vec<LadderEntry>, EngineError> {
    quantities
        .iter()
        .map(|&qty| {
            let mut s = spec.clone();
            s.quantity = qty;
            let bd = price_job(model, policy, &s, margin_override)?;
            Ok(LadderEntry {
                qty,
                total_minor: bd.total_minor,
                unit_minor: bd.unit_minor,
            })
        })
        .collect()
}

/// Template ladder quantities first, then validated custom quantities appended
/// (deduplicated, ascending) — §6.4.
fn ladder_quantities(
    template: &ProductTemplate,
    requested: Option<&[u32]>,
) -> Result<Vec<u32>, EngineError> {
    let mut list = template.quantities.clone();
    if let Some(requested) = requested {
        let mut extra = Vec::new();
        for &qty in requested {
            match template.custom_quantity {
                Some(CustomQuantity { min, max }) if qty >= min && qty <= max => {}
                _ => {
                    let (min, max) = template
                        .custom_quantity
                        .map(|c| (c.min, c.max))
                        .unwrap_or((0, 0));
                    return Err(EngineError::InvalidQuantity { qty, min, max });
                }
            }
            if !list.contains(&qty) && !extra.contains(&qty) {
                extra.push(qty);
            }
        }
        extra.sort_unstable();
        list.extend(extra);
    }
    Ok(list)
}
