//! Engine error taxonomy. The `E1xx`/`E2xx` codes are the spec's error
//! registry (§7); the API layer maps [`EngineError`] variants onto the HTTP
//! error codes (`INVALID_SELECTION`, `RULE_VIOLATION`, `TEMPLATE_ERROR`,
//! `UNPRICEABLE`, …).

use crate::model::I18n;

/// Resolution / template errors (`E101`–`E110`). Surface as `TEMPLATE_ERROR`
/// (500) when raised against a saved template — lint should prevent them.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResolveError {
    #[error("E101 unknown target role: {0}")]
    UnknownRole(String),
    #[error("E102 duplicate add_operation: {0}")]
    DuplicateOperation(String),
    #[error("E103 set_op_param on absent operation: {0}")]
    OpParamOnAbsentOperation(String),
    #[error("E104 add_component on existing role: {0}")]
    ComponentExists(String),
    #[error("E105 empty technology allow-list")]
    EmptyTechnologyAllow,
    #[error("E106 incomplete spec: {0}")]
    IncompleteSpec(String),
    #[error("E107 dangling record reference: {0}")]
    DanglingReference(String),
    #[error("E108 numeric default violates input constraints: {0}")]
    BadNumericDefault(String),
    #[error("E110 $input outside a numeric parameter's effects")]
    InputOutsideNumeric,
}

impl ResolveError {
    /// The `E1xx` code string, for logs and the API `TEMPLATE_ERROR` detail.
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownRole(_) => "E101",
            Self::DuplicateOperation(_) => "E102",
            Self::OpParamOnAbsentOperation(_) => "E103",
            Self::ComponentExists(_) => "E104",
            Self::EmptyTechnologyAllow => "E105",
            Self::IncompleteSpec(_) => "E106",
            Self::DanglingReference(_) => "E107",
            Self::BadNumericDefault(_) => "E108",
            Self::InputOutsideNumeric => "E110",
        }
    }
}

/// Pricing errors (`E2xx`). Surface as `UNPRICEABLE` (500) — lint should
/// prevent them. `E203` is folded into capability (no capable machine → `E201`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PriceError {
    #[error("E201 no capable machine for component: {0}")]
    NoCapableMachine(String),
    #[error("E202 item larger than sheet for component: {0}")]
    ItemLargerThanSheet(String),
    #[error("E204 material basis mismatch on operation: {0}")]
    MaterialBasisMismatch(String),
    #[error("E205 invalid reserved param on operation: {0}")]
    InvalidReservedParam(String),
}

impl PriceError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoCapableMachine(_) => "E201",
            Self::ItemLargerThanSheet(_) => "E202",
            Self::MaterialBasisMismatch(_) => "E204",
            Self::InvalidReservedParam(_) => "E205",
        }
    }
}

/// Why a selection failed §4.1 validation. Maps to `INVALID_SELECTION` with the
/// parameter code and this reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionReason {
    UnknownParameter,
    MissingNoDefault,
    UnknownOption,
    OptionUnavailable,
    OutOfRange,
}

impl SelectionReason {
    pub fn code(&self) -> &'static str {
        match self {
            Self::UnknownParameter => "unknown_parameter",
            Self::MissingNoDefault => "missing_no_default",
            Self::UnknownOption => "unknown_option",
            Self::OptionUnavailable => "option_unavailable",
            Self::OutOfRange => "out_of_range",
        }
    }
}

/// One violated compatibility rule, returned localized (§5, §7 `RULE_VIOLATION`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleViolation {
    pub rule_id: String,
    pub message: I18n,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineError {
    #[error("invalid selection for parameter {parameter}: {}", reason.code())]
    InvalidSelection {
        parameter: String,
        reason: SelectionReason,
    },
    #[error("invalid quantity {qty} (allowed {min}..={max})")]
    InvalidQuantity { qty: u32, min: u32, max: u32 },
    #[error(transparent)]
    Resolve(#[from] ResolveError),
    #[error("{} rule(s) violated", .0.len())]
    RuleViolation(Vec<RuleViolation>),
    #[error(transparent)]
    Price(#[from] PriceError),
}
