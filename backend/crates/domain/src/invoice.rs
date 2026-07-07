use chrono::{Duration, NaiveDate};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::error::DomainError;
use crate::money::{Money, round_half_up};
use crate::order::LineItem;

/// Default VAT rate (19%, in basis points) applied to every invoice in the
/// prototype — not yet client-configurable (see PLAN.md M2 scope).
pub const DEFAULT_TAX_RATE_BP: u32 = 1900;

/// Default invoice payment term: `due_date = issue_date + 14 days`.
pub const INVOICE_DUE_DAYS: i64 = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    Draft,
    Issued,
    Paid,
    Void,
}

/// Invoice status transitions per PLAN.md: `draft -> issued -> paid`; `void`
/// reachable from `draft`/`issued`. An invalid transition is a `409
/// conflict`, not a validation error.
pub fn validate_transition(
    current: InvoiceStatus,
    target: InvoiceStatus,
) -> Result<(), DomainError> {
    use InvoiceStatus::*;
    let allowed = match current {
        Draft => matches!(target, Issued | Void),
        Issued => matches!(target, Paid | Void),
        Paid | Void => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(DomainError::Conflict(format!(
            "cannot transition invoice from {current:?} to {target:?}"
        )))
    }
}

/// `round(net_total × tax_rate_bp / 10000)`, half-up, on the total — not
/// per line, per PLAN.md.
pub fn compute_tax(net_total: &Money, tax_rate_bp: u32) -> Money {
    let amount_minor = round_half_up(net_total.amount_minor * tax_rate_bp as i64, 10_000);
    Money {
        amount_minor,
        currency: net_total.currency.clone(),
    }
}

pub fn compute_gross(net_total: &Money, tax_total: &Money) -> Money {
    Money {
        amount_minor: net_total.amount_minor + tax_total.amount_minor,
        currency: net_total.currency.clone(),
    }
}

pub fn due_date_from_issue(issue_date: NaiveDate) -> NaiveDate {
    issue_date + Duration::days(INVOICE_DUE_DAYS)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Invoice {
    pub id: String,
    pub number: String,
    pub order_id: String,
    pub customer_id: String,
    pub status: InvoiceStatus,
    pub currency: String,
    pub exchange_rate: Option<String>,
    pub line_items: Vec<LineItem>,
    pub net_total: Money,
    pub tax_rate_bp: u32,
    pub tax_total: Money,
    pub gross_total: Money,
    pub issue_date: Option<String>,
    pub due_date: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewInvoice {
    #[validate(length(min = 1, message = "must not be empty"))]
    pub order_id: String,
    /// Optional currency override. M2 only accepts a value equal to the
    /// order's own currency — real multi-currency invoicing (rate snapshot,
    /// display conversion) is M5 scope per PLAN.md.
    pub currency: Option<String>,
}

impl NewInvoice {
    pub fn validate_domain(&self) -> Result<(), DomainError> {
        self.validate().map_err(DomainError::from)
    }
}

#[derive(Debug, Clone)]
pub struct InvoiceListQuery {
    pub page: u32,
    pub limit: u32,
    pub sort: String,
    pub customer_id: Option<String>,
    pub status: Option<InvoiceStatus>,
    /// Full-text filter (M3). When present, results are ranked by BM25
    /// score and `sort` is ignored, per PLAN.md's list-parameters contract.
    pub q: Option<String>,
}

#[async_trait::async_trait]
pub trait InvoiceRepo: Send + Sync {
    async fn list(&self, query: InvoiceListQuery) -> Result<crate::Paged<Invoice>, DomainError>;
    async fn get(&self, id: &str) -> Result<Option<Invoice>, DomainError>;
    async fn create(&self, data: NewInvoice) -> Result<Invoice, DomainError>;
    async fn update(&self, id: &str, data: NewInvoice) -> Result<Invoice, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;
    async fn set_status(&self, id: &str, status: InvoiceStatus) -> Result<Invoice, DomainError>;
    /// Top BM25-ranked hits for the global omnibox (M3). `q` is assumed
    /// non-empty — callers filter that out before calling.
    async fn search(&self, q: &str, limit: u32) -> Result<Vec<crate::SearchHit>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn money(amount_minor: i64) -> Money {
        Money {
            amount_minor,
            currency: "EUR".to_string(),
        }
    }

    #[test]
    fn tax_rounds_down_below_half() {
        let net = money(333);
        let tax = compute_tax(&net, DEFAULT_TAX_RATE_BP);
        // 333 * 1900 / 10000 = 63.27 -> 63
        assert_eq!(tax.amount_minor, 63);
    }

    #[test]
    fn tax_rounds_up_at_exact_half() {
        let net = money(50);
        let tax = compute_tax(&net, DEFAULT_TAX_RATE_BP);
        // 50 * 1900 / 10000 = 9.5 -> 10 (half-up)
        assert_eq!(tax.amount_minor, 10);
    }

    #[test]
    fn tax_with_no_rounding_needed() {
        let net = money(1000);
        let tax = compute_tax(&net, DEFAULT_TAX_RATE_BP);
        // 1000 * 1900 / 10000 = 190.0 exactly
        assert_eq!(tax.amount_minor, 190);
    }

    #[test]
    fn gross_is_net_plus_tax() {
        let net = money(1000);
        let tax = compute_tax(&net, DEFAULT_TAX_RATE_BP);
        let gross = compute_gross(&net, &tax);
        assert_eq!(gross.amount_minor, 1190);
        assert_eq!(gross.currency, "EUR");
    }

    #[test]
    fn due_date_is_fourteen_days_after_issue() {
        let issue = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let due = due_date_from_issue(issue);
        assert_eq!(due, NaiveDate::from_ymd_opt(2026, 1, 15).unwrap());
    }

    #[test]
    fn draft_can_be_issued_or_voided() {
        assert!(validate_transition(InvoiceStatus::Draft, InvoiceStatus::Issued).is_ok());
        assert!(validate_transition(InvoiceStatus::Draft, InvoiceStatus::Void).is_ok());
    }

    #[test]
    fn draft_cannot_skip_to_paid() {
        let err = validate_transition(InvoiceStatus::Draft, InvoiceStatus::Paid).unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[test]
    fn paid_and_void_are_terminal() {
        assert!(validate_transition(InvoiceStatus::Paid, InvoiceStatus::Void).is_err());
        assert!(validate_transition(InvoiceStatus::Void, InvoiceStatus::Draft).is_err());
    }
}
