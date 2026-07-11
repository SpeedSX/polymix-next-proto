use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::error::{ConflictReason, DomainError, FieldError};
use crate::money::Money;
use crate::tenant::Tenant;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderStatus {
    Draft,
    Confirmed,
    InProduction,
    Completed,
    Cancelled,
}

/// Whether `status` allows an invoice to be raised against the order — used
/// by the invoice repo, kept here since it's an order-status invariant.
pub fn can_invoice(status: OrderStatus) -> bool {
    matches!(
        status,
        OrderStatus::Confirmed | OrderStatus::InProduction | OrderStatus::Completed
    )
}

/// Order status transitions per PLAN.md: `draft -> confirmed -> in_production
/// -> completed`; `cancelled` reachable from `draft`/`confirmed`; no other
/// moves. An invalid transition is a `409 conflict`, not a validation error.
pub fn validate_transition(current: OrderStatus, target: OrderStatus) -> Result<(), DomainError> {
    use OrderStatus::*;
    let allowed = match current {
        Draft => matches!(target, Confirmed | Cancelled),
        Confirmed => matches!(target, InProduction | Cancelled),
        InProduction => matches!(target, Completed),
        Completed | Cancelled => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(DomainError::Conflict(ConflictReason::OrderStatusTransition {
            from: current,
            to: target,
        }))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct LineItem {
    #[validate(length(min = 1, code = "required"))]
    pub description: String,
    #[validate(range(min = 1, code = "positive_quantity"))]
    pub quantity: u32,
    #[validate(nested)]
    pub unit_price: Money,
}

/// Sums `quantity × unit_price` across line items. Every line item's price
/// must already be in `currency` — callers resolve/validate that before
/// calling this so the sum is never a currency mismatch.
pub fn line_items_total(line_items: &[LineItem], currency: &str) -> Money {
    let amount_minor = line_items
        .iter()
        .map(|item| item.unit_price.amount_minor * item.quantity as i64)
        .sum();
    Money {
        amount_minor,
        currency: currency.to_string(),
    }
}

/// Validates that every line item's price is denominated in `currency`.
/// Kept separate from `line_items_total` so the sum stays a pure function.
pub fn validate_line_item_currencies(
    line_items: &[LineItem],
    currency: &str,
) -> Result<(), DomainError> {
    let mismatched = line_items
        .iter()
        .any(|item| item.unit_price.currency != currency);
    if mismatched {
        let mut details = HashMap::new();
        details.insert(
            "line_items".to_string(),
            FieldError::with_params(
                "currency_mismatch",
                HashMap::from([("currency".to_string(), currency.to_string())]),
            ),
        );
        return Err(DomainError::Validation(details));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub number: String,
    pub customer_id: String,
    /// Resolved from the customer record at read time — not stored on the
    /// order. `None` when the referenced customer no longer exists.
    pub customer_name: Option<String>,
    pub status: OrderStatus,
    pub currency: String,
    pub line_items: Vec<LineItem>,
    pub total: Money,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct NewOrder {
    #[validate(length(min = 1, code = "required"))]
    pub customer_id: String,
    pub currency: Option<String>,
    #[validate(length(min = 1, code = "min_line_items"), nested)]
    pub line_items: Vec<LineItem>,
    pub notes: Option<String>,
}

impl NewOrder {
    pub fn validate_domain(&self) -> Result<(), DomainError> {
        self.validate().map_err(DomainError::from)
    }

    /// Fills `currency` from the tenant's default when the client omitted
    /// it, per PLAN.md ("currency ... defaults to tenant default
    /// currency"). Call before `validate_domain`/`line_items_total` so both
    /// see the resolved value.
    pub fn resolve_currency(&mut self, tenant_default_currency: &str) {
        if self.currency.is_none() {
            self.currency = Some(tenant_default_currency.to_string());
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderListQuery {
    pub page: u32,
    pub limit: u32,
    pub sort: String,
    pub customer_id: Option<String>,
    pub status: Option<OrderStatus>,
    /// Full-text filter (M3). When present, results are ranked by BM25
    /// score and `sort` is ignored, per PLAN.md's list-parameters contract.
    pub q: Option<String>,
}

#[async_trait::async_trait]
pub trait OrderRepo: Send + Sync {
    async fn list(&self, query: OrderListQuery) -> Result<crate::Paged<Order>, DomainError>;
    async fn get(&self, id: &str) -> Result<Option<Order>, DomainError>;
    /// `tenant` supplies `order_prefix` for the assigned number (PLAN.md M4).
    async fn create(&self, data: NewOrder, tenant: &Tenant) -> Result<Order, DomainError>;
    async fn update(&self, id: &str, data: NewOrder) -> Result<Order, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;
    async fn set_status(&self, id: &str, status: OrderStatus) -> Result<Order, DomainError>;
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

    fn line_item(quantity: u32, unit_price_minor: i64) -> LineItem {
        LineItem {
            description: "Business cards".to_string(),
            quantity,
            unit_price: money(unit_price_minor),
        }
    }

    #[test]
    fn total_sums_quantity_times_unit_price() {
        let items = vec![line_item(3, 250), line_item(2, 1000)];
        let total = line_items_total(&items, "EUR");
        assert_eq!(total.amount_minor, 3 * 250 + 2 * 1000);
        assert_eq!(total.currency, "EUR");
    }

    #[test]
    fn empty_line_items_is_invalid() {
        let data = NewOrder {
            customer_id: "customer1".to_string(),
            currency: None,
            line_items: vec![],
            notes: None,
        };
        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("line_items"));
    }

    #[test]
    fn zero_quantity_is_invalid() {
        let data = NewOrder {
            customer_id: "customer1".to_string(),
            currency: None,
            line_items: vec![line_item(0, 100)],
            notes: None,
        };
        let err = data.validate_domain().unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("line_items[0].quantity"));
    }

    #[test]
    fn resolve_currency_fills_only_when_absent() {
        let mut with_default = NewOrder {
            customer_id: "customer1".to_string(),
            currency: None,
            line_items: vec![line_item(1, 100)],
            notes: None,
        };
        with_default.resolve_currency("EUR");
        assert_eq!(with_default.currency.as_deref(), Some("EUR"));

        let mut with_explicit = NewOrder {
            customer_id: "customer1".to_string(),
            currency: Some("USD".to_string()),
            line_items: vec![line_item(1, 100)],
            notes: None,
        };
        with_explicit.resolve_currency("EUR");
        assert_eq!(with_explicit.currency.as_deref(), Some("USD"));
    }

    #[test]
    fn mismatched_line_item_currency_is_rejected() {
        let items = vec![LineItem {
            description: "Flyers".to_string(),
            quantity: 1,
            unit_price: Money {
                amount_minor: 100,
                currency: "USD".to_string(),
            },
        }];
        let err = validate_line_item_currencies(&items, "EUR").unwrap_err();
        let DomainError::Validation(details) = err else {
            panic!("expected Validation error");
        };
        assert!(details.contains_key("line_items"));
    }

    #[test]
    fn draft_can_confirm_or_cancel() {
        assert!(validate_transition(OrderStatus::Draft, OrderStatus::Confirmed).is_ok());
        assert!(validate_transition(OrderStatus::Draft, OrderStatus::Cancelled).is_ok());
    }

    #[test]
    fn draft_cannot_skip_to_in_production() {
        let err = validate_transition(OrderStatus::Draft, OrderStatus::InProduction).unwrap_err();
        assert!(matches!(err, DomainError::Conflict(_)));
    }

    #[test]
    fn completed_is_terminal() {
        assert!(validate_transition(OrderStatus::Completed, OrderStatus::Cancelled).is_err());
    }

    #[test]
    fn cancelled_is_terminal() {
        assert!(validate_transition(OrderStatus::Cancelled, OrderStatus::Draft).is_err());
    }

    #[test]
    fn only_confirmed_or_later_can_be_invoiced() {
        assert!(!can_invoice(OrderStatus::Draft));
        assert!(can_invoice(OrderStatus::Confirmed));
        assert!(can_invoice(OrderStatus::InProduction));
        assert!(can_invoice(OrderStatus::Completed));
        assert!(!can_invoice(OrderStatus::Cancelled));
    }
}
