use std::collections::HashMap;
use std::fmt;

use serde::Serialize;

use crate::customer::CustomerStatus;
use crate::invoice::InvoiceStatus;
use crate::order::OrderStatus;
use crate::quote::QuoteStatus;

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
    CustomerModified,
    CustomerNotActiveForOrder,
    OrderHasInvoice,
    OrderNotConfirmedForInvoice,
    OrderAlreadyInvoiced,
    InvoiceNotDraft,
    InvoiceCannotBeDeleted,
    QuoteNotDraft,
    QuoteNotAccepted,
    QuoteAlreadyConverted,
    QuoteExpired,
    QuoteConvertRequiresCustomer,
    OrderStatusTransition {
        from: OrderStatus,
        to: OrderStatus,
    },
    QuoteStatusTransition {
        from: QuoteStatus,
        to: QuoteStatus,
    },
    InvoiceStatusTransition {
        from: InvoiceStatus,
        to: InvoiceStatus,
    },
    CustomerStatusTransition {
        from: CustomerStatus,
        to: CustomerStatus,
    },
}

impl ConflictReason {
    pub fn code(&self) -> &'static str {
        match self {
            Self::CustomerHasOrders => "customer_has_orders",
            Self::CustomerModified => "customer_modified",
            Self::CustomerNotActiveForOrder => "customer_not_active_for_order",
            Self::OrderHasInvoice => "order_has_invoice",
            Self::OrderNotConfirmedForInvoice => "order_not_confirmed_for_invoice",
            Self::OrderAlreadyInvoiced => "order_already_invoiced",
            Self::InvoiceNotDraft => "invoice_not_draft",
            Self::InvoiceCannotBeDeleted => "invoice_cannot_be_deleted",
            Self::QuoteNotDraft => "quote_not_draft",
            Self::QuoteNotAccepted => "quote_not_accepted",
            Self::QuoteAlreadyConverted => "quote_already_converted",
            Self::QuoteExpired => "quote_expired",
            Self::QuoteConvertRequiresCustomer => "quote_convert_requires_customer",
            Self::OrderStatusTransition { .. } => "order_status_transition",
            Self::QuoteStatusTransition { .. } => "quote_status_transition",
            Self::InvoiceStatusTransition { .. } => "invoice_status_transition",
            Self::CustomerStatusTransition { .. } => "customer_status_transition",
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
            Self::QuoteStatusTransition { from, to } => Some(HashMap::from([
                ("from".to_string(), status_code(from)),
                ("to".to_string(), status_code(to)),
            ])),
            Self::InvoiceStatusTransition { from, to } => Some(HashMap::from([
                ("from".to_string(), status_code(from)),
                ("to".to_string(), status_code(to)),
            ])),
            Self::CustomerStatusTransition { from, to } => Some(HashMap::from([
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
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                return Some(s.to_string());
            }
            if let Some(n) = v.as_i64() {
                return Some(n.to_string());
            }
            None
        })
        .unwrap_or_default()
}

impl fmt::Display for ConflictReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CustomerHasOrders => write!(f, "customer has orders and cannot be deleted"),
            Self::CustomerModified => write!(
                f,
                "customer was modified by someone else since it was loaded"
            ),
            Self::CustomerNotActiveForOrder => write!(f, "customer is not active"),
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
            Self::QuoteNotDraft => write!(
                f,
                "quote can only be edited while in draft status; clone it to revise"
            ),
            Self::QuoteNotAccepted => {
                write!(
                    f,
                    "quote must be accepted before it can be converted to an order"
                )
            }
            Self::QuoteAlreadyConverted => {
                write!(f, "quote has already been converted to an order")
            }
            Self::QuoteExpired => write!(f, "quote has expired"),
            Self::QuoteConvertRequiresCustomer => {
                write!(
                    f,
                    "prospect quote must be assigned a customer before conversion"
                )
            }
            Self::OrderStatusTransition { from, to } => {
                write!(f, "cannot transition order from {from:?} to {to:?}")
            }
            Self::QuoteStatusTransition { from, to } => {
                write!(f, "cannot transition quote from {from:?} to {to:?}")
            }
            Self::InvoiceStatusTransition { from, to } => {
                write!(f, "cannot transition invoice from {from:?} to {to:?}")
            }
            Self::CustomerStatusTransition { from, to } => {
                write!(f, "cannot transition customer from {from:?} to {to:?}")
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

#[cfg(test)]
mod tests {
    use super::*;
    use validator::Validate;

    #[test]
    fn field_error_code_starts_with_no_params() {
        let err = FieldError::code("required");
        assert_eq!(err.code, "required");
        assert!(err.params.is_empty());
    }

    #[test]
    fn field_error_with_params_keeps_them() {
        let params = HashMap::from([("max".to_string(), "10".to_string())]);
        let err = FieldError::with_params("too_long", params);
        assert_eq!(err.code, "too_long");
        assert_eq!(err.params.get("max").map(String::as_str), Some("10"));
    }

    #[test]
    fn empty_params_are_skipped_when_serialized() {
        let json = serde_json::to_value(FieldError::code("required")).unwrap();
        assert_eq!(json["code"], "required");
        assert!(json.get("params").is_none());
    }

    #[test]
    fn non_empty_params_are_serialized() {
        let err = FieldError::with_params(
            "out_of_range",
            HashMap::from([("min".to_string(), "0".to_string())]),
        );
        let json = serde_json::to_value(err).unwrap();
        assert_eq!(json["params"]["min"], "0");
    }

    #[test]
    fn conflict_codes_are_stable_and_unique() {
        let reasons = [
            ConflictReason::CustomerHasOrders,
            ConflictReason::CustomerModified,
            ConflictReason::CustomerNotActiveForOrder,
            ConflictReason::OrderHasInvoice,
            ConflictReason::OrderNotConfirmedForInvoice,
            ConflictReason::OrderAlreadyInvoiced,
            ConflictReason::InvoiceNotDraft,
            ConflictReason::InvoiceCannotBeDeleted,
            ConflictReason::OrderStatusTransition {
                from: OrderStatus::Draft,
                to: OrderStatus::Completed,
            },
            ConflictReason::InvoiceStatusTransition {
                from: InvoiceStatus::Draft,
                to: InvoiceStatus::Paid,
            },
            ConflictReason::CustomerStatusTransition {
                from: CustomerStatus::Lead,
                to: CustomerStatus::Blocked,
            },
        ];
        let codes: Vec<&str> = reasons.iter().map(|r| r.code()).collect();
        let unique: std::collections::HashSet<&str> = codes.iter().copied().collect();
        assert_eq!(codes.len(), unique.len(), "conflict codes must be unique");
        assert!(codes.iter().all(|c| !c.is_empty()));
    }

    #[test]
    fn non_transition_conflicts_have_no_details() {
        assert!(ConflictReason::CustomerHasOrders.details().is_none());
        assert!(ConflictReason::OrderHasInvoice.details().is_none());
        assert!(ConflictReason::InvoiceCannotBeDeleted.details().is_none());
    }

    #[test]
    fn order_transition_details_use_numeric_status_codes() {
        // OrderStatus serializes as a u8 code, so `status_code` takes the
        // integer branch and stringifies the code.
        let details = ConflictReason::OrderStatusTransition {
            from: OrderStatus::Draft,
            to: OrderStatus::Completed,
        }
        .details()
        .expect("transition reasons carry details");
        assert_eq!(details.get("from").map(String::as_str), Some("0"));
        assert_eq!(details.get("to").map(String::as_str), Some("3"));
    }

    #[test]
    fn invoice_transition_details_use_string_status_codes() {
        // InvoiceStatus serializes as a snake_case string, exercising the
        // string branch of `status_code`.
        let details = ConflictReason::InvoiceStatusTransition {
            from: InvoiceStatus::Draft,
            to: InvoiceStatus::Issued,
        }
        .details()
        .expect("transition reasons carry details");
        assert_eq!(details.get("from").map(String::as_str), Some("draft"));
        assert_eq!(details.get("to").map(String::as_str), Some("issued"));
    }

    #[test]
    fn customer_transition_details_use_numeric_status_codes() {
        let details = ConflictReason::CustomerStatusTransition {
            from: CustomerStatus::Lead,
            to: CustomerStatus::Blocked,
        }
        .details()
        .expect("transition reasons carry details");
        assert_eq!(details.get("from").map(String::as_str), Some("0"));
        assert_eq!(details.get("to").map(String::as_str), Some("3"));
    }

    #[test]
    fn display_mentions_the_status_names_for_transitions() {
        let reason = ConflictReason::OrderStatusTransition {
            from: OrderStatus::Draft,
            to: OrderStatus::Completed,
        };
        let text = reason.to_string();
        assert!(text.contains("Draft"), "got: {text}");
        assert!(text.contains("Completed"), "got: {text}");
    }

    #[test]
    fn display_is_human_readable_for_simple_conflicts() {
        assert_eq!(
            ConflictReason::CustomerHasOrders.to_string(),
            "customer has orders and cannot be deleted"
        );
        assert!(
            ConflictReason::OrderAlreadyInvoiced
                .to_string()
                .contains("already has an invoice")
        );
    }

    #[test]
    fn domain_error_display_wraps_the_conflict_reason() {
        let err = DomainError::Conflict(ConflictReason::CustomerModified);
        assert_eq!(
            err.to_string(),
            "conflict: customer was modified by someone else since it was loaded"
        );
        assert_eq!(DomainError::NotFound.to_string(), "not found");
        assert_eq!(
            DomainError::Store("boom".to_string()).to_string(),
            "store error: boom"
        );
    }

    #[derive(Validate)]
    struct Leaf {
        #[validate(length(min = 1, code = "required"))]
        name: String,
    }

    #[derive(Validate)]
    struct Branch {
        #[validate(nested)]
        leaf: Leaf,
        #[validate(nested)]
        leaves: Vec<Leaf>,
    }

    #[test]
    fn from_validation_errors_flattens_top_level_field() {
        let leaf = Leaf {
            name: String::new(),
        };
        let DomainError::Validation(details) = DomainError::from(leaf.validate().unwrap_err())
        else {
            panic!("expected a validation error");
        };
        assert_eq!(
            details.get("name").map(|f| f.code.as_str()),
            Some("required")
        );
    }

    #[test]
    fn from_validation_errors_flattens_nested_struct_and_list_paths() {
        let branch = Branch {
            leaf: Leaf {
                name: String::new(),
            },
            leaves: vec![
                Leaf {
                    name: "ok".to_string(),
                },
                Leaf {
                    name: String::new(),
                },
            ],
        };
        let DomainError::Validation(details) = DomainError::from(branch.validate().unwrap_err())
        else {
            panic!("expected a validation error");
        };
        // Nested struct field is flattened to a dotted path...
        assert_eq!(
            details.get("leaf.name").map(|f| f.code.as_str()),
            Some("required")
        );
        // ...and list entries carry their index, only the failing one.
        assert_eq!(
            details.get("leaves[1].name").map(|f| f.code.as_str()),
            Some("required")
        );
        assert!(!details.contains_key("leaves[0].name"));
    }
}
