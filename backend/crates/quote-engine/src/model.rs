//! §2 — the `PriceModel` data schema.
//!
//! Field names here are normative for both these Rust structs and the stored
//! JSON. Record ids are SurrealDB-style strings (`format:a5`,
//! `material:offset_80`, …). The engine never mutates a model; it is loaded
//! once from a [`Dataset`] and read.

use std::collections::BTreeMap;
use std::fmt;

use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::effect::Effect;
use crate::rules::CompatibilityRule;

/// Localized label map (`{"en": "A5", "uk": "А5"}`). Presentation only — the
/// engine never branches on it. `BTreeMap` keeps serialization deterministic.
pub type I18n = BTreeMap<String, String>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Technology {
    Digital,
    Offset,
}

/// The four unit bases shared by operations and material pricing (§2). A
/// material's pricing basis must equal an operation's `unit_basis` when the
/// material is consumed by that operation (`E204`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnitBasis {
    PerItem,
    PerSheet,
    PerCm,
    PerM2,
}

/// Front/back ink counts (`4/0`, `4/4`, `1/1`). Serializes as the `"F/B"`
/// string form (§3 `ColorsSpec`); both integers are `0..=8`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Colors {
    pub front: u8,
    pub back: u8,
}

impl Colors {
    pub const UNPRINTED: Colors = Colors { front: 0, back: 0 };

    pub fn is_unprinted(&self) -> bool {
        self.front == 0 && self.back == 0
    }
}

impl Serialize for Colors {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format!("{}/{}", self.front, self.back))
    }
}

impl<'de> Deserialize<'de> for Colors {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct ColorsVisitor;
        impl Visitor<'_> for ColorsVisitor {
            type Value = Colors;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(r#"a colors string "F/B" with F,B in 0..=8"#)
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Colors, E> {
                parse_colors(v).ok_or_else(|| E::custom(format!("invalid colors spec: {v:?}")))
            }
        }
        d.deserialize_str(ColorsVisitor)
    }
}

/// Parse the `"F/B"` colors form; `None` on any other shape (§3: regex
/// `^([0-8])/([0-8])$`).
fn parse_colors(v: &str) -> Option<Colors> {
    let (f, b) = v.split_once('/')?;
    let front: u8 = f.parse().ok()?;
    let back: u8 = b.parse().ok()?;
    (front <= 8 && back <= 8).then_some(Colors { front, back })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Format {
    pub id: String,
    pub name: String,
    /// `[width, height]` in mm, stored portrait (`width <= height`).
    pub trim_mm: [u32; 2],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Material {
    pub id: String,
    pub name: String,
    /// UI classification only — free taxonomy, the engine never branches on it.
    pub kind: String,
    pub pricing: MaterialPricing,
    /// Present only for substrates a press can print on; read by machine
    /// capability checks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub printable: Option<Printable>,
    /// Opaque descriptive metadata for admin UI / production docs.
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub attrs: serde_json::Map<String, serde_json::Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Printable {
    pub grammage_gsm: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "basis", rename_all = "snake_case")]
pub enum MaterialPricing {
    PerSheet {
        sheet_size_mm: [u32; 2],
        price_micro: i64,
    },
    PerM2 {
        price_micro: i64,
    },
    PerCm {
        price_micro: i64,
    },
    PerItem {
        price_micro: i64,
    },
}

impl MaterialPricing {
    pub fn price_micro(&self) -> i64 {
        match self {
            Self::PerSheet { price_micro, .. }
            | Self::PerM2 { price_micro }
            | Self::PerCm { price_micro }
            | Self::PerItem { price_micro } => *price_micro,
        }
    }

    pub fn basis(&self) -> UnitBasis {
        match self {
            Self::PerSheet { .. } => UnitBasis::PerSheet,
            Self::PerM2 { .. } => UnitBasis::PerM2,
            Self::PerCm { .. } => UnitBasis::PerCm,
            Self::PerItem { .. } => UnitBasis::PerItem,
        }
    }

    /// The press-sheet size for `per_sheet` materials — the only basis
    /// imposition can consume (`E107`).
    pub fn sheet_size_mm(&self) -> Option<[u32; 2]> {
        match self {
            Self::PerSheet { sheet_size_mm, .. } => Some(*sheet_size_mm),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub name: String,
    pub technology: Technology,
    pub sheet_size_mm: [u32; 2],
    pub duplex: bool,
    pub max_grammage_gsm: u32,
    pub setup_micro: i64,
    pub waste_fixed_sheets: u32,
    /// Whole percent (`2` = 2 %).
    pub waste_percent: u32,
    // Digital only.
    #[serde(default)]
    pub click_mono_micro: i64,
    #[serde(default)]
    pub click_color_micro: i64,
    // Offset only.
    #[serde(default)]
    pub plate_price_micro: i64,
    #[serde(default)]
    pub run_price_micro: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Operation {
    pub id: String,
    #[serde(default)]
    pub name: String,
    pub setup_micro: i64,
    pub unit_basis: UnitBasis,
    pub unit_price_micro: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PricingPolicy {
    pub id: String,
    pub currency: String,
    /// Sorted by `min_qty` ascending; the first band must have `min_qty` 1.
    pub margin_bands: Vec<MarginBand>,
    pub rounding: Rounding,
    pub min_price_minor: i64,
}

impl PricingPolicy {
    /// The band with the largest `min_qty <= qty` (§2 band selection).
    pub fn band_for(&self, qty: u32) -> Option<&MarginBand> {
        self.margin_bands
            .iter()
            .filter(|b| b.min_qty <= qty)
            .max_by_key(|b| b.min_qty)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarginBand {
    pub min_qty: u32,
    /// Basis points: `17000` = ×1.7.
    pub multiplier_bp: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rounding {
    pub step_minor: i64,
    pub mode: RoundingMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoundingMode {
    /// v1: rounding is always up.
    Up,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductTemplate {
    pub id: String,
    pub slug: String,
    pub name: I18n,
    /// Component roles that exist before any effect runs.
    pub components: Vec<String>,
    pub pricing_policy: String,
    pub quantities: Vec<u32>,
    /// `None` → custom quantity not offered.
    #[serde(default)]
    pub custom_quantity: Option<CustomQuantity>,
    #[serde(default)]
    pub base_effects: Vec<Effect>,
    #[serde(default)]
    pub parameters: Vec<Parameter>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomQuantity {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Parameter {
    pub code: String,
    #[serde(default)]
    pub label: I18n,
    #[serde(flatten)]
    pub kind: ParameterKind,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ParameterKind {
    Select {
        options: Vec<OptionDef>,
    },
    Numeric {
        input: NumericInput,
        #[serde(default)]
        default: Option<u32>,
        #[serde(default)]
        effects: Vec<Effect>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptionDef {
    pub code: String,
    #[serde(default)]
    pub label: I18n,
    #[serde(default)]
    pub is_default: bool,
    /// Standing admin flag: option temporarily not fulfillable. Default true.
    #[serde(default = "default_true")]
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unavailable_message: Option<I18n>,
    #[serde(default)]
    pub effects: Vec<Effect>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct NumericInput {
    pub min: u32,
    pub max: u32,
    pub step: u32,
}

fn default_true() -> bool {
    true
}

/// Deserialization shape for a whole tenant price model — flat arrays as stored
/// / seeded. [`PriceModel::from_dataset`] indexes it for lookup.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Dataset {
    #[serde(default = "default_version")]
    pub pricelist_version: i64,
    #[serde(default)]
    pub formats: Vec<Format>,
    #[serde(default)]
    pub materials: Vec<Material>,
    #[serde(default)]
    pub machines: Vec<Machine>,
    #[serde(default)]
    pub operations: Vec<Operation>,
    #[serde(default)]
    pub pricing_policies: Vec<PricingPolicy>,
    #[serde(default)]
    pub templates: Vec<ProductTemplate>,
    #[serde(default)]
    pub compatibility_rules: Vec<CompatibilityRule>,
}

fn default_version() -> i64 {
    1
}

/// Indexed, read-only tenant price model. All maps are `BTreeMap` so every
/// derived iteration (machine selection tie-breaks, serialization) is
/// deterministic (§1).
#[derive(Debug, Clone)]
pub struct PriceModel {
    pub pricelist_version: i64,
    pub formats: BTreeMap<String, Format>,
    pub materials: BTreeMap<String, Material>,
    pub machines: BTreeMap<String, Machine>,
    pub operations: BTreeMap<String, Operation>,
    pub pricing_policies: BTreeMap<String, PricingPolicy>,
    pub templates: BTreeMap<String, ProductTemplate>,
    templates_by_slug: BTreeMap<String, String>,
    rules_by_template: BTreeMap<String, Vec<CompatibilityRule>>,
}

impl PriceModel {
    pub fn from_dataset(ds: Dataset) -> Self {
        let templates_by_slug = ds
            .templates
            .iter()
            .map(|t| (t.slug.clone(), t.id.clone()))
            .collect();
        let mut rules_by_template: BTreeMap<String, Vec<CompatibilityRule>> = BTreeMap::new();
        for rule in ds.compatibility_rules {
            rules_by_template
                .entry(rule.template.clone())
                .or_default()
                .push(rule);
        }
        Self {
            pricelist_version: ds.pricelist_version,
            formats: ds.formats.into_iter().map(|f| (f.id.clone(), f)).collect(),
            materials: ds
                .materials
                .into_iter()
                .map(|m| (m.id.clone(), m))
                .collect(),
            machines: ds.machines.into_iter().map(|m| (m.id.clone(), m)).collect(),
            operations: ds
                .operations
                .into_iter()
                .map(|o| (o.id.clone(), o))
                .collect(),
            pricing_policies: ds
                .pricing_policies
                .into_iter()
                .map(|p| (p.id.clone(), p))
                .collect(),
            templates: ds
                .templates
                .into_iter()
                .map(|t| (t.id.clone(), t))
                .collect(),
            templates_by_slug,
            rules_by_template,
        }
    }

    pub fn template_by_slug(&self, slug: &str) -> Option<&ProductTemplate> {
        self.templates_by_slug
            .get(slug)
            .and_then(|id| self.templates.get(id))
    }

    pub fn rules_for(&self, template_id: &str) -> &[CompatibilityRule] {
        self.rules_by_template
            .get(template_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colors_round_trip_through_string() {
        let c = Colors { front: 4, back: 0 };
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(json, "\"4/0\"");
        assert_eq!(
            serde_json::from_str::<Colors>("\"4/4\"").unwrap(),
            Colors { front: 4, back: 4 }
        );
    }

    #[test]
    fn colors_rejects_bad_shapes() {
        for bad in ["\"9/0\"", "\"4\"", "\"4/4/4\"", "\"x/0\"", "\"4-0\""] {
            assert!(
                serde_json::from_str::<Colors>(bad).is_err(),
                "expected {bad} to be rejected"
            );
        }
    }

    #[test]
    fn band_selection_picks_largest_min_qty_not_exceeding_qty() {
        let policy = PricingPolicy {
            id: "pricing_policy:standard".into(),
            currency: "EUR".into(),
            margin_bands: vec![
                MarginBand {
                    min_qty: 1,
                    multiplier_bp: 17000,
                },
                MarginBand {
                    min_qty: 250,
                    multiplier_bp: 16000,
                },
                MarginBand {
                    min_qty: 1000,
                    multiplier_bp: 15000,
                },
            ],
            rounding: Rounding {
                step_minor: 10,
                mode: RoundingMode::Up,
            },
            min_price_minor: 2500,
        };
        assert_eq!(policy.band_for(1).unwrap().multiplier_bp, 17000);
        assert_eq!(policy.band_for(100).unwrap().multiplier_bp, 17000);
        assert_eq!(policy.band_for(250).unwrap().multiplier_bp, 16000);
        assert_eq!(policy.band_for(999).unwrap().multiplier_bp, 16000);
        assert_eq!(policy.band_for(1000).unwrap().multiplier_bp, 15000);
        assert_eq!(policy.band_for(5000).unwrap().multiplier_bp, 15000);
    }
}
