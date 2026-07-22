//! Pricing catalog domain surface (A2a).
//!
//! The stored and response shapes ARE the `quote-engine` `PriceModel` structs
//! (`docs/pricing-admin-plan.md` pinned decision: reuse, don't define parallel
//! DTOs). CRUD flows carry catalog documents as `serde_json::Value` so a single
//! repo/route pair serves all five tables; the engine structs are the normative
//! schema and [`validate`] enforces the spec's structural constraints on write
//! so malformed data can't enter the model at all.

use std::collections::HashMap;

use async_trait::async_trait;
use quote_engine::{Dataset, Format, Machine, Material, Operation, PricingPolicy, Technology};
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::error::{DomainError, FieldError};

/// The five catalog tables a tenant admin edits. Every CRUD path is keyed by
/// one of these; the URL segment ↔ table mapping lives here so the routes and
/// the repo can't drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PricingEntity {
    Format,
    Material,
    Machine,
    Operation,
    Policy,
}

impl PricingEntity {
    /// Resolve a `/api/pricing/{segment}` path segment to an entity.
    pub fn from_segment(segment: &str) -> Option<Self> {
        Some(match segment {
            "formats" => Self::Format,
            "materials" => Self::Material,
            "machines" => Self::Machine,
            "operations" => Self::Operation,
            "policies" => Self::Policy,
            _ => return None,
        })
    }

    /// The SurrealDB table backing this entity (also the record-id prefix).
    pub fn table(self) -> &'static str {
        match self {
            Self::Format => "format",
            Self::Material => "material",
            Self::Machine => "machine",
            Self::Operation => "operation",
            Self::Policy => "pricing_policy",
        }
    }

    /// Field to sort a list by. `pricing_policy` has no `name` in the normative
    /// schema (spec §2), so it sorts by currency instead.
    pub fn sort_field(self) -> &'static str {
        match self {
            Self::Policy => "currency",
            _ => "name",
        }
    }

    pub const ALL: [PricingEntity; 5] = [
        Self::Format,
        Self::Material,
        Self::Machine,
        Self::Operation,
        Self::Policy,
    ];
}

/// Tenant-scoped CRUD over the pricing catalog. Documents are the engine's
/// stored shapes as JSON; every mutation bumps `meta:pricing.version` in the
/// same transaction (spec §2), which is the implementation's responsibility.
#[async_trait]
pub trait PricingRepo: Send + Sync {
    async fn list(&self, entity: PricingEntity) -> Result<Vec<Value>, DomainError>;
    async fn get(&self, entity: PricingEntity, id: &str) -> Result<Option<Value>, DomainError>;
    async fn create(&self, entity: PricingEntity, doc: Value) -> Result<Value, DomainError>;
    async fn update(
        &self,
        entity: PricingEntity,
        id: &str,
        doc: Value,
    ) -> Result<Value, DomainError>;
    async fn delete(&self, entity: PricingEntity, id: &str) -> Result<(), DomainError>;
    /// Current `meta:pricing.version` — the audit anchor quotes are priced
    /// against and the snapshot cache invalidates on.
    async fn get_version(&self) -> Result<i64, DomainError>;
    /// The whole catalog as a [`Dataset`] for the in-memory `PriceModel`
    /// snapshot, tagged with the version it was read at.
    async fn load_dataset(&self) -> Result<Dataset, DomainError>;
}

fn errors(map: HashMap<String, FieldError>) -> Result<(), DomainError> {
    if map.is_empty() {
        Ok(())
    } else {
        Err(DomainError::Validation(map))
    }
}

/// Deserialize a catalog document into its engine struct, mapping a shape
/// mismatch (wrong types, a `material.pricing` payload that doesn't match its
/// `basis`, an unknown `unit_basis`, …) to a single top-level field error the
/// frontend renders as a form-level message.
fn parse<T: DeserializeOwned>(doc: &Value) -> Result<T, DomainError> {
    serde_json::from_value::<T>(doc.clone()).map_err(|_| {
        DomainError::Validation(HashMap::from([(
            "_".to_string(),
            FieldError::code("invalid_shape"),
        )]))
    })
}

/// Enforce the spec's structural constraints for `entity` on a write payload
/// (`docs/pricing-admin-plan.md` A2a-2). The `id` field is expected to be
/// present already (the repo injects it before validating).
pub fn validate(entity: PricingEntity, doc: &Value) -> Result<(), DomainError> {
    match entity {
        PricingEntity::Format => validate_format(doc),
        PricingEntity::Material => validate_material(doc),
        PricingEntity::Machine => validate_machine(doc),
        PricingEntity::Operation => validate_operation(doc),
        PricingEntity::Policy => validate_policy(doc),
    }
}

fn validate_format(doc: &Value) -> Result<(), DomainError> {
    let format: Format = parse(doc)?;
    let [w, h] = format.trim_mm;
    let mut e = HashMap::new();
    if w == 0 || h == 0 {
        e.insert(
            "trim_mm".to_string(),
            FieldError::code("positive_dimensions"),
        );
    } else if w > h {
        // Formats are stored portrait: width <= height (spec §1).
        e.insert("trim_mm".to_string(), FieldError::code("portrait_required"));
    }
    errors(e)
}

fn validate_material(doc: &Value) -> Result<(), DomainError> {
    let material: Material = parse(doc)?;
    let mut e = HashMap::new();
    if material.pricing.price_micro() < 0 {
        e.insert("pricing".to_string(), FieldError::code("non_negative"));
    }
    if let Some(printable) = &material.printable
        && printable.grammage_gsm == 0
    {
        e.insert(
            "printable".to_string(),
            FieldError::code("positive_grammage"),
        );
    }
    errors(e)
}

fn validate_machine(doc: &Value) -> Result<(), DomainError> {
    let m: Machine = parse(doc)?;
    let mut e = HashMap::new();
    // A machine prices either by digital clicks or by offset plates/run — never
    // both, and each technology must carry its own cost fields (spec §2).
    match m.technology {
        Technology::Digital => {
            if m.click_mono_micro <= 0 {
                e.insert(
                    "click_mono_micro".to_string(),
                    FieldError::code("required_for_digital"),
                );
            }
            if m.click_color_micro <= 0 {
                e.insert(
                    "click_color_micro".to_string(),
                    FieldError::code("required_for_digital"),
                );
            }
            if m.plate_price_micro != 0 {
                e.insert(
                    "plate_price_micro".to_string(),
                    FieldError::code("not_for_digital"),
                );
            }
            if m.run_price_micro != 0 {
                e.insert(
                    "run_price_micro".to_string(),
                    FieldError::code("not_for_digital"),
                );
            }
        }
        Technology::Offset => {
            if m.plate_price_micro <= 0 {
                e.insert(
                    "plate_price_micro".to_string(),
                    FieldError::code("required_for_offset"),
                );
            }
            if m.run_price_micro <= 0 {
                e.insert(
                    "run_price_micro".to_string(),
                    FieldError::code("required_for_offset"),
                );
            }
            if m.click_mono_micro != 0 {
                e.insert(
                    "click_mono_micro".to_string(),
                    FieldError::code("not_for_offset"),
                );
            }
            if m.click_color_micro != 0 {
                e.insert(
                    "click_color_micro".to_string(),
                    FieldError::code("not_for_offset"),
                );
            }
        }
    }
    errors(e)
}

fn validate_operation(doc: &Value) -> Result<(), DomainError> {
    // `unit_basis` is validated against the closed set by the enum's serde.
    let op: Operation = parse(doc)?;
    let mut e = HashMap::new();
    if op.setup_micro < 0 {
        e.insert("setup_micro".to_string(), FieldError::code("non_negative"));
    }
    if op.unit_price_micro < 0 {
        e.insert(
            "unit_price_micro".to_string(),
            FieldError::code("non_negative"),
        );
    }
    errors(e)
}

fn validate_policy(doc: &Value) -> Result<(), DomainError> {
    // `rounding.mode` is pinned to "up" by the `RoundingMode` enum's serde.
    let policy: PricingPolicy = parse(doc)?;
    let mut e = HashMap::new();
    if policy.currency.len() != 3 {
        e.insert("currency".to_string(), FieldError::code("invalid_currency"));
    }
    if policy.margin_bands.is_empty() {
        e.insert(
            "margin_bands".to_string(),
            FieldError::code("bands_required"),
        );
    } else {
        if policy.margin_bands[0].min_qty != 1 {
            e.insert(
                "margin_bands".to_string(),
                FieldError::code("first_band_min_qty_one"),
            );
        }
        let ascending = policy
            .margin_bands
            .windows(2)
            .all(|w| w[0].min_qty < w[1].min_qty);
        if !ascending {
            e.entry("margin_bands".to_string())
                .or_insert_with(|| FieldError::code("bands_ascending"));
        }
        if policy.margin_bands.iter().any(|b| b.multiplier_bp == 0) {
            e.entry("margin_bands".to_string())
                .or_insert_with(|| FieldError::code("positive_multiplier"));
        }
    }
    if policy.rounding.step_minor <= 0 {
        e.insert("rounding".to_string(), FieldError::code("positive_step"));
    }
    if policy.min_price_minor < 0 {
        e.insert(
            "min_price_minor".to_string(),
            FieldError::code("non_negative"),
        );
    }
    errors(e)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn segment_round_trips_to_table() {
        assert_eq!(
            PricingEntity::from_segment("formats"),
            Some(PricingEntity::Format)
        );
        assert_eq!(
            PricingEntity::from_segment("policies"),
            Some(PricingEntity::Policy)
        );
        assert_eq!(PricingEntity::from_segment("widgets"), None);
        assert_eq!(PricingEntity::Policy.table(), "pricing_policy");
    }

    #[test]
    fn format_portrait_invariant() {
        let ok = json!({ "id": "format:a5", "name": "A5", "trim_mm": [148, 210] });
        assert!(validate(PricingEntity::Format, &ok).is_ok());

        let landscape = json!({ "id": "format:dl", "name": "DL", "trim_mm": [210, 99] });
        assert!(matches!(
            validate(PricingEntity::Format, &landscape),
            Err(DomainError::Validation(e)) if e["trim_mm"].code == "portrait_required"
        ));

        let zero = json!({ "id": "format:x", "name": "X", "trim_mm": [0, 210] });
        assert!(matches!(
            validate(PricingEntity::Format, &zero),
            Err(DomainError::Validation(e)) if e["trim_mm"].code == "positive_dimensions"
        ));
    }

    #[test]
    fn material_printable_grammage_and_shape() {
        let ok = json!({ "id": "material:offset_80", "name": "Offset 80 g", "kind": "paper",
            "pricing": { "basis": "per_sheet", "sheet_size_mm": [320, 450], "price_micro": 40000 },
            "printable": { "grammage_gsm": 80 } });
        assert!(validate(PricingEntity::Material, &ok).is_ok());

        let bad_grammage = json!({ "id": "material:x", "name": "X", "kind": "paper",
            "pricing": { "basis": "per_sheet", "sheet_size_mm": [320, 450], "price_micro": 40000 },
            "printable": { "grammage_gsm": 0 } });
        assert!(matches!(
            validate(PricingEntity::Material, &bad_grammage),
            Err(DomainError::Validation(e)) if e["printable"].code == "positive_grammage"
        ));

        // per_sheet basis with no sheet_size_mm is a shape mismatch.
        let bad_shape = json!({ "id": "material:x", "name": "X", "kind": "paper",
            "pricing": { "basis": "per_sheet", "price_micro": 40000 } });
        assert!(matches!(
            validate(PricingEntity::Material, &bad_shape),
            Err(DomainError::Validation(e)) if e.contains_key("_")
        ));
    }

    #[test]
    fn machine_technology_cost_split() {
        let digital = json!({ "id": "machine:digi1", "name": "Digital", "technology": "digital",
            "sheet_size_mm": [320, 450], "duplex": true, "max_grammage_gsm": 350,
            "setup_micro": 2000000, "click_mono_micro": 8000, "click_color_micro": 60000,
            "waste_fixed_sheets": 10, "waste_percent": 2 });
        assert!(validate(PricingEntity::Machine, &digital).is_ok());

        let digital_with_plates = json!({ "id": "machine:x", "name": "X", "technology": "digital",
            "sheet_size_mm": [320, 450], "duplex": true, "max_grammage_gsm": 350,
            "setup_micro": 2000000, "click_mono_micro": 8000, "click_color_micro": 60000,
            "plate_price_micro": 5000, "waste_fixed_sheets": 10, "waste_percent": 2 });
        assert!(matches!(
            validate(PricingEntity::Machine, &digital_with_plates),
            Err(DomainError::Validation(e)) if e["plate_price_micro"].code == "not_for_digital"
        ));

        let offset_missing = json!({ "id": "machine:offset1", "name": "Offset", "technology": "offset",
            "sheet_size_mm": [320, 450], "duplex": true, "max_grammage_gsm": 400,
            "setup_micro": 15000000, "run_price_micro": 15000,
            "waste_fixed_sheets": 150, "waste_percent": 2 });
        assert!(matches!(
            validate(PricingEntity::Machine, &offset_missing),
            Err(DomainError::Validation(e)) if e["plate_price_micro"].code == "required_for_offset"
        ));
    }

    #[test]
    fn policy_band_rules() {
        let ok = json!({ "id": "pricing_policy:standard", "currency": "EUR",
            "margin_bands": [ { "min_qty": 1, "multiplier_bp": 17000 },
                              { "min_qty": 250, "multiplier_bp": 16000 } ],
            "rounding": { "step_minor": 10, "mode": "up" }, "min_price_minor": 2500 });
        assert!(validate(PricingEntity::Policy, &ok).is_ok());

        let bad_first = json!({ "id": "pricing_policy:x", "currency": "EUR",
            "margin_bands": [ { "min_qty": 5, "multiplier_bp": 17000 } ],
            "rounding": { "step_minor": 10, "mode": "up" }, "min_price_minor": 2500 });
        assert!(matches!(
            validate(PricingEntity::Policy, &bad_first),
            Err(DomainError::Validation(e)) if e["margin_bands"].code == "first_band_min_qty_one"
        ));

        let not_ascending = json!({ "id": "pricing_policy:x", "currency": "EUR",
            "margin_bands": [ { "min_qty": 1, "multiplier_bp": 17000 },
                              { "min_qty": 1, "multiplier_bp": 16000 } ],
            "rounding": { "step_minor": 10, "mode": "up" }, "min_price_minor": 2500 });
        assert!(matches!(
            validate(PricingEntity::Policy, &not_ascending),
            Err(DomainError::Validation(e)) if e["margin_bands"].code == "bands_ascending"
        ));
    }
}
