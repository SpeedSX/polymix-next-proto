use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use crate::invoice::InvoiceStatus;
use crate::order::OrderStatus;

#[derive(Debug, Clone, thiserror::Error)]
pub enum DomainError {
    #[error("not found")]
    NotFound,
    #[error("validation failed")]
    Validation(HashMap<String, FieldError>),
    #[error("conflict: {0}")]
    Conflict(ConflictReason),
    #[error("store error: {0}")]
    Store(String),
}

/// Stable, localization-friendly reason a single field failed validation —
/// the field-level analogue of [`ConflictReason`]. `code` is what the
/// frontend keys off of to pick a translated message; `params` carries any
/// dynamic data (e.g. the invalid value) the translated message needs to
/// interpolate, so nothing English-specific has to travel over the wire.
#[derive(Debug, Clone, Serialize)]
pub struct FieldError {
    pub code: String,
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub params: HashMap<String, String>,
}

impl FieldError {
    pub fn code(code: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            params: HashMap::new(),
        }
    }

    pub fn with_params(code: impl Into<String>, params: HashMap<String, String>) -> Self {
        Self {
            code: code.into(),
            params,
        }
    }
}

/// Stable, localization-friendly reason for a [`DomainError::Conflict`].
/// `code()` is what the API and frontend key off of to pick a translated
/// message; `Display` (below) stays English-only and backs the API's
/// `message` field and server logs, not the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictReason {
    CustomerHasOrders,
    OrderHasInvoice,
    OrderNotConfirmedForInvoice,
    OrderAlreadyInvoiced,
    InvoiceNotDraft,
    InvoiceCannotBeDeleted,
    OrderStatusTransition {
        from: OrderStatus,
        to: OrderStatus,
    },
    InvoiceStatusTransition {
        from: InvoiceStatus,
        to: InvoiceStatus,
    },
}

impl ConflictReason {
    pub fn code(&self) -> &'static str {
        match self {
            Self::CustomerHasOrders => "customer_has_orders",
            Self::OrderHasInvoice => "order_has_invoice",
            Self::OrderNotConfirmedForInvoice => "order_not_confirmed_for_invoice",
            Self::OrderAlreadyInvoiced => "order_already_invoiced",
            Self::InvoiceNotDraft => "invoice_not_draft",
            Self::InvoiceCannotBeDeleted => "invoice_cannot_be_deleted",
            Self::OrderStatusTransition { .. } => "order_status_transition",
            Self::InvoiceStatusTransition { .. } => "invoice_status_transition",
        }
    }

    /// Machine-readable payload for codes carrying dynamic data — lets the
    /// frontend render a fully localized message (e.g. via its own
    /// status-label translations) instead of parsing status names out of
    /// the English `message` string.
    pub fn details(&self) -> Option<HashMap<String, String>> {
        match self {
            Self::OrderStatusTransition { from, to } => Some(HashMap::from([
                ("from".to_string(), status_code(from)),
                ("to".to_string(), status_code(to)),
            ])),
            Self::InvoiceStatusTransition { from, to } => Some(HashMap::from([
                ("from".to_string(), status_code(from)),
                ("to".to_string(), status_code(to)),
            ])),
            _ => None,
        }
    }
}

fn status_code<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap_or_default()
}

impl fmt::Display for ConflictReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CustomerHasOrders => write!(f, "customer has orders and cannot be deleted"),
            Self::OrderHasInvoice => write!(f, "order has an invoice and cannot be deleted"),
            Self::OrderNotConfirmedForInvoice => {
                write!(f, "order must be confirmed before it can be invoiced")
            }
            Self::OrderAlreadyInvoiced => write!(f, "order already has an invoice"),
            Self::InvoiceNotDraft => write!(
                f,
                "invoice can only be edited while in draft status; void and reissue instead"
            ),
            Self::InvoiceCannotBeDeleted => {
                write!(f, "invoices cannot be deleted; void them instead")
            }
            Self::OrderStatusTransition { from, to } => {
                write!(f, "cannot transition order from {from:?} to {to:?}")
            }
            Self::InvoiceStatusTransition { from, to } => {
                write!(f, "cannot transition invoice from {from:?} to {to:?}")
            }
        }
    }
}

impl From<validator::ValidationErrors> for DomainError {
    fn from(errors: validator::ValidationErrors) -> Self {
        let mut details = HashMap::new();
        flatten_validation_errors("", &errors, &mut details);
        DomainError::Validation(details)
    }
}

/// `ValidationErrors::field_errors` only sees direct fields, dropping errors from
/// `#[validate(nested)]` structs — flatten those into dotted paths (e.g.
/// `address.country`) so nested validation failures still reach the API response.
fn flatten_validation_errors(
    prefix: &str,
    errors: &validator::ValidationErrors,
    out: &mut HashMap<String, FieldError>,
) {
    for (field, kind) in errors.errors() {
        let key = if prefix.is_empty() {
            field.to_string()
        } else {
            format!("{prefix}.{field}")
        };
        match kind {
            validator::ValidationErrorsKind::Field(errs) => {
                if let Some(err) = errs.first() {
                    out.insert(key, FieldError::code(err.code.to_string()));
                }
            }
            validator::ValidationErrorsKind::Struct(nested) => {
                flatten_validation_errors(&key, nested, out);
            }
            validator::ValidationErrorsKind::List(list) => {
                for (idx, nested) in list {
                    flatten_validation_errors(&format!("{key}[{idx}]"), nested, out);
                }
            }
        }
    }
}
