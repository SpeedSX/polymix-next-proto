//! PolyMix quote engine — a pure, deterministic pricing crate.
//!
//! Normative spec: `docs/quote-engine-spec.md`. Same `PriceModel` + same
//! request ⇒ byte-identical response. No I/O, no async, no floats.
//!
//! Pipeline: [`resolve`] turns a template + [`Selection`] into a [`JobSpec`]
//! (§4); [`rules::violations`] checks compatibility rules (§5);
//! [`price_job`]/[`quote_template`] cost it (§6).

pub mod effect;
pub mod error;
pub mod model;
pub mod money;
pub mod price;
pub mod resolve;
pub mod rules;

pub use effect::{ColorsSpec, Effect, InputExpr, NumOrInput};
pub use error::{EngineError, PriceError, ResolveError, RuleViolation, SelectionReason};
pub use model::{
    Colors, CustomQuantity, Dataset, Format, I18n, Machine, MarginBand, Material, MaterialPricing,
    NumericInput, Operation, OptionDef, Parameter, ParameterKind, PriceModel, PricingPolicy,
    Printable, ProductTemplate, Rounding, RoundingMode, Technology, UnitBasis,
};
pub use price::{
    Breakdown, ComponentCost, LadderEntry, OperationCost, Quote, price_at, price_job, quote_spec,
    quote_template, ups,
};
pub use resolve::{Component, JobSpec, OperationInstance, Selection, resolve};
pub use rules::{CmpOp, CompatibilityRule, Condition, violations};
