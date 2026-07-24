//! Quote documents and staff estimating (`docs/staff-quoting.md`).
//!
//! A quote is a counter-numbered, tenant-scoped document with embedded lines
//! priced in one of three tiers (template / direct spec / manual) and a
//! commercial lifecycle. Engine pricing is computed in the API layer (which
//! holds the price-model snapshot) and stored on each line as [`EnginePricing`];
//! the repo only persists what it is given — see the route layer for the
//! spec→breakdown step.

use serde::de::Error as _;
use serde::{Deserialize, Serialize};

use quote_engine::{Breakdown, JobSpec, Selection};

use crate::error::{ConflictReason, DomainError, FieldError};
use crate::tenant::Tenant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuoteStatus {
    Draft,
    Sent,
    Accepted,
    Declined,
    Expired,
}

impl QuoteStatus {
    pub const fn code(self) -> u8 {
        match self {
            Self::Draft => 0,
            Self::Sent => 1,
            Self::Accepted => 2,
            Self::Declined => 3,
            Self::Expired => 4,
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::Accepted => "accepted",
            Self::Declined => "declined",
            Self::Expired => "expired",
        }
    }

    pub const fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Draft),
            1 => Some(Self::Sent),
            2 => Some(Self::Accepted),
            3 => Some(Self::Declined),
            4 => Some(Self::Expired),
            _ => None,
        }
    }
}

impl Serialize for QuoteStatus {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(self.code())
    }
}

impl<'de> Deserialize<'de> for QuoteStatus {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = u8::deserialize(deserializer)?;
        Self::from_code(code)
            .ok_or_else(|| D::Error::custom(format!("invalid quote status code: {code}")))
    }
}

/// Only drafts are content-mutable; a sent quote is frozen and revised by
/// cloning (`docs/staff-quoting.md` lifecycle).
pub fn can_edit(status: QuoteStatus) -> bool {
    matches!(status, QuoteStatus::Draft)
}

/// Quote lifecycle transitions: `draft → sent → accepted`; `sent → declined |
/// expired`; `accepted → expired`. Order conversion is a separate action, not a
/// status change (an accepted quote keeps its status and gains an `order`
/// link). An invalid transition is a `409 conflict`.
pub fn validate_transition(current: QuoteStatus, target: QuoteStatus) -> Result<(), DomainError> {
    use QuoteStatus::*;
    let allowed = match current {
        Draft => matches!(target, Sent),
        Sent => matches!(target, Accepted | Declined | Expired),
        Accepted => matches!(target, Expired),
        Declined | Expired => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(DomainError::Conflict(
            ConflictReason::QuoteStatusTransition {
                from: current,
                to: target,
            },
        ))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AdjustmentKind {
    /// Replaces the policy band multiplier (basis points, must be > 0).
    MarginOverride { multiplier_bp: u32 },
    /// Off the engine price (basis points, 0..=10_000).
    Discount { percent_bp: u32 },
    /// Manual final price in minor units (must be >= 0); engine price kept.
    PriceOverride { total_minor: i64 },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Adjustment {
    #[serde(flatten)]
    pub kind: AdjustmentKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Adjustment {
    fn validate(&self, field: &str) -> Result<(), DomainError> {
        let ok = match self.kind {
            AdjustmentKind::MarginOverride { multiplier_bp } => multiplier_bp > 0,
            AdjustmentKind::Discount { percent_bp } => percent_bp <= 10_000,
            AdjustmentKind::PriceOverride { total_minor } => total_minor >= 0,
        };
        if ok {
            Ok(())
        } else {
            Err(validation(field, "invalid_adjustment"))
        }
    }

    /// Applies the adjustment to an engine total, yielding the final total in
    /// minor units. `MarginOverride` is priced by the engine (the caller passes
    /// its result as `engine_total_minor` already re-priced); here it is a
    /// passthrough. `Discount`/`PriceOverride` are applied to the engine result
    /// per spec delta 3.
    pub fn apply(&self, engine_total_minor: i64) -> i64 {
        match self.kind {
            AdjustmentKind::MarginOverride { .. } => engine_total_minor,
            AdjustmentKind::Discount { percent_bp } => {
                engine_total_minor - (engine_total_minor * percent_bp as i64) / 10_000
            }
            AdjustmentKind::PriceOverride { total_minor } => total_minor,
        }
    }
}

/// Stored pricing snapshot for an engine-priced line — the audit record that
/// makes a quote reproducible (`docs/staff-quoting.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnginePricing {
    pub breakdown: Breakdown,
    /// What §6.4 produced with the policy's own band (no adjustment).
    pub engine_total_minor: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub adjustment: Option<Adjustment>,
    /// `engine_total_minor` with the adjustment applied.
    pub final_total_minor: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum QuoteLine {
    Template {
        line_id: String,
        template: String,
        selection: Selection,
        qty: u32,
        pricing: EnginePricing,
    },
    Spec {
        line_id: String,
        job_spec: JobSpec,
        description: String,
        qty: u32,
        pricing: EnginePricing,
    },
    Manual {
        line_id: String,
        description: String,
        qty: u32,
        unit_minor: i64,
    },
}

impl QuoteLine {
    pub fn line_id(&self) -> &str {
        match self {
            Self::Template { line_id, .. }
            | Self::Spec { line_id, .. }
            | Self::Manual { line_id, .. } => line_id,
        }
    }

    /// The line's contribution to the quote total in minor units.
    pub fn total_minor(&self) -> i64 {
        match self {
            Self::Template { pricing, .. } | Self::Spec { pricing, .. } => {
                pricing.final_total_minor
            }
            Self::Manual {
                qty, unit_minor, ..
            } => *qty as i64 * *unit_minor,
        }
    }
}

/// Sum of every line's total in minor units, in the quote currency.
pub fn quote_total_minor(lines: &[QuoteLine]) -> i64 {
    lines.iter().map(QuoteLine::total_minor).sum()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prospect {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Quote {
    pub id: String,
    pub number: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_id: Option<String>,
    /// Resolved from the customer record at read time — not stored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customer_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prospect: Option<Prospect>,
    pub currency: String,
    pub status: QuoteStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    pub lines: Vec<QuoteLine>,
    /// Set iff the quote has ≥1 engine-priced line.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pricelist_version: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    pub created_by: String,
    /// Back-link to the quote this one was cloned from (the revision chain).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revises: Option<String>,
    /// Set once converted; one order per quote.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub order_id: Option<String>,
    /// Total of all lines in minor units, in `currency`.
    pub total_minor: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// One line of a `POST`/`PUT` quote body — the client submits specs and
/// adjustments, never prices; the API prices engine lines server-side.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewQuoteLine {
    Template {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        line_id: Option<String>,
        template: String,
        selection: Selection,
        qty: u32,
        #[serde(default)]
        adjustment: Option<Adjustment>,
    },
    Spec {
        #[serde(default)]
        line_id: Option<String>,
        job_spec: JobSpec,
        description: String,
        qty: u32,
        #[serde(default)]
        adjustment: Option<Adjustment>,
    },
    Manual {
        #[serde(default)]
        line_id: Option<String>,
        description: String,
        qty: u32,
        unit_minor: i64,
    },
}

impl NewQuoteLine {
    fn validate(&self, index: usize) -> Result<(), DomainError> {
        let field = |name: &str| format!("lines[{index}].{name}");
        let qty = match self {
            Self::Template { qty, .. } | Self::Spec { qty, .. } | Self::Manual { qty, .. } => *qty,
        };
        if qty < 1 {
            return Err(validation(&field("qty"), "positive_quantity"));
        }
        match self {
            Self::Manual { unit_minor, .. } if *unit_minor < 0 => {
                return Err(validation(&field("unit_minor"), "non_negative"));
            }
            Self::Template {
                adjustment: Some(adj),
                ..
            }
            | Self::Spec {
                adjustment: Some(adj),
                ..
            } => adj.validate(&field("adjustment"))?,
            _ => {}
        }
        Ok(())
    }

    pub fn adjustment(&self) -> Option<&Adjustment> {
        match self {
            Self::Template { adjustment, .. } | Self::Spec { adjustment, .. } => {
                adjustment.as_ref()
            }
            Self::Manual { .. } => None,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NewQuote {
    #[serde(default)]
    pub customer_id: Option<String>,
    #[serde(default)]
    pub prospect: Option<Prospect>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub valid_until: Option<String>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub lines: Vec<NewQuoteLine>,
}

impl NewQuote {
    /// Structural validation independent of pricing: exactly one party, and
    /// each line well-formed. Currency resolution and party existence are the
    /// caller's job.
    pub fn validate_domain(&self) -> Result<(), DomainError> {
        match (self.customer_id.as_deref(), self.prospect.as_ref()) {
            (Some(id), None) if !id.is_empty() => {}
            (None, Some(p)) if !p.name.trim().is_empty() => {}
            _ => return Err(validation("customer_id", "party_required")),
        }
        for (index, line) in self.lines.iter().enumerate() {
            line.validate(index)?;
        }
        Ok(())
    }

    pub fn resolve_currency(&mut self, tenant_default_currency: &str) {
        if self.currency.is_none() {
            self.currency = Some(tenant_default_currency.to_string());
        }
    }
}

fn validation(field: &str, code: &str) -> DomainError {
    DomainError::Validation(std::collections::HashMap::from([(
        field.to_string(),
        FieldError::code(code),
    )]))
}

/// One order line produced by converting a quote line: quantity `qty` at
/// `unit_minor`, carrying the source `line_id` for traceability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConvertedLine {
    pub description: String,
    pub qty: u32,
    pub unit_minor: i64,
    pub source_line_id: String,
}

/// Splits an engine-priced line's `final_total_minor` across `qty` units
/// without changing the accepted total (`docs/staff-quoting.md` quote→order).
/// `final = qty*base + remainder` with `0 <= remainder < qty`: a line of
/// `qty - remainder` units at `base`, plus (when `remainder > 0`) a second line
/// of `remainder` units at `base + 1`. Both share the source `line_id`.
pub fn split_residual_minor(
    description: &str,
    qty: u32,
    final_total_minor: i64,
    line_id: &str,
) -> Vec<ConvertedLine> {
    let q = qty as i64;
    let base = final_total_minor.div_euclid(q);
    let remainder = final_total_minor.rem_euclid(q);
    let mut lines = Vec::with_capacity(2);
    let whole = q - remainder;
    if whole > 0 {
        lines.push(ConvertedLine {
            description: description.to_string(),
            qty: whole as u32,
            unit_minor: base,
            source_line_id: line_id.to_string(),
        });
    }
    if remainder > 0 {
        lines.push(ConvertedLine {
            description: description.to_string(),
            qty: remainder as u32,
            unit_minor: base + 1,
            source_line_id: line_id.to_string(),
        });
    }
    lines
}

#[derive(Debug, Clone)]
pub struct QuoteListQuery {
    pub page: u32,
    pub limit: u32,
    pub sort: String,
    pub customer_id: Option<String>,
    pub status: Option<QuoteStatus>,
    pub q: Option<String>,
}

/// Fully-priced create/update payload the API hands the repo: engine lines
/// already carry their [`EnginePricing`]. `pricelist_version` is `Some` iff any
/// line is engine-priced.
#[derive(Debug, Clone)]
pub struct QuoteWrite {
    pub customer_id: Option<String>,
    pub prospect: Option<Prospect>,
    pub currency: String,
    pub valid_until: Option<String>,
    pub notes: Option<String>,
    pub lines: Vec<QuoteLine>,
    pub pricelist_version: Option<i64>,
    pub created_by: String,
}

#[async_trait::async_trait]
pub trait QuoteRepo: Send + Sync {
    async fn list(&self, query: QuoteListQuery) -> Result<crate::Paged<Quote>, DomainError>;
    async fn get(&self, id: &str) -> Result<Option<Quote>, DomainError>;
    /// `tenant` supplies `quote_prefix` for the assigned number.
    async fn create(&self, data: QuoteWrite, tenant: &Tenant) -> Result<Quote, DomainError>;
    /// Draft-only; a non-draft quote is a `409` conflict. `data` preserves the
    /// original `created_by` — the caller reads it off the existing quote.
    async fn update(&self, id: &str, data: QuoteWrite) -> Result<Quote, DomainError>;
    async fn delete(&self, id: &str) -> Result<(), DomainError>;
    async fn set_status(&self, id: &str, status: QuoteStatus) -> Result<Quote, DomainError>;
    /// Clone any quote into a fresh draft with a new number and a `revises`
    /// back-link, owned by `created_by`. Line ids are preserved. (Named
    /// `clone_quote`, not `clone`, so it never shadows `Arc::clone` on a
    /// trait-object handle.)
    async fn clone_quote(
        &self,
        id: &str,
        tenant: &Tenant,
        created_by: &str,
    ) -> Result<Quote, DomainError>;
    /// Convert an accepted, unexpired, unconverted quote into a draft order,
    /// linking the two. Re-checks `valid_until` against `now` and expires the
    /// quote (returning [`ConflictReason::QuoteExpired`]) if it has passed.
    async fn convert_to_order(
        &self,
        id: &str,
        tenant: &Tenant,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<crate::Order, DomainError>;
    async fn search(&self, q: &str, limit: u32) -> Result<Vec<crate::SearchHit>, DomainError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draft_only_sends() {
        assert!(validate_transition(QuoteStatus::Draft, QuoteStatus::Sent).is_ok());
        assert!(validate_transition(QuoteStatus::Draft, QuoteStatus::Accepted).is_err());
    }

    #[test]
    fn sent_can_accept_decline_or_expire() {
        for target in [
            QuoteStatus::Accepted,
            QuoteStatus::Declined,
            QuoteStatus::Expired,
        ] {
            assert!(validate_transition(QuoteStatus::Sent, target).is_ok());
        }
    }

    #[test]
    fn accepted_only_expires() {
        assert!(validate_transition(QuoteStatus::Accepted, QuoteStatus::Expired).is_ok());
        assert!(validate_transition(QuoteStatus::Accepted, QuoteStatus::Declined).is_err());
    }

    #[test]
    fn declined_and_expired_are_terminal() {
        assert!(validate_transition(QuoteStatus::Declined, QuoteStatus::Sent).is_err());
        assert!(validate_transition(QuoteStatus::Expired, QuoteStatus::Sent).is_err());
    }

    #[test]
    fn only_drafts_edit() {
        assert!(can_edit(QuoteStatus::Draft));
        assert!(!can_edit(QuoteStatus::Sent));
    }

    #[test]
    fn discount_applies_off_engine_total() {
        let adj = Adjustment {
            kind: AdjustmentKind::Discount { percent_bp: 1200 },
            reason: None,
        };
        // 834_65 minor with 12% off -> floor(83465 * 1200 / 10000) = 10015 off.
        assert_eq!(adj.apply(83_465), 83_465 - 10_015);
    }

    #[test]
    fn price_override_replaces_total() {
        let adj = Adjustment {
            kind: AdjustmentKind::PriceOverride {
                total_minor: 180_000,
            },
            reason: Some("goodwill".into()),
        };
        assert_eq!(adj.apply(205_000), 180_000);
    }

    #[test]
    fn residual_split_preserves_total_when_divisible() {
        let total = 200 * 500;
        let lines = split_residual_minor("Book", 500, total, "l1");
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].qty, 500);
        assert_eq!(lines[0].unit_minor, 200);
        let sum: i64 = lines.iter().map(|l| l.qty as i64 * l.unit_minor).sum();
        assert_eq!(sum, total);
    }

    #[test]
    fn residual_split_allocates_remainder_to_second_line() {
        // 1000 total over 3 units -> 333*2 + 334*1 = 1000.
        let lines = split_residual_minor("Slipcase", 3, 1000, "l2");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].qty, 2);
        assert_eq!(lines[0].unit_minor, 333);
        assert_eq!(lines[1].qty, 1);
        assert_eq!(lines[1].unit_minor, 334);
        let sum: i64 = lines.iter().map(|l| l.qty as i64 * l.unit_minor).sum();
        assert_eq!(sum, 1000);
        assert!(lines.iter().all(|l| l.source_line_id == "l2"));
    }

    #[test]
    fn party_required_rejects_both_or_neither() {
        let neither = NewQuote {
            customer_id: None,
            prospect: None,
            currency: None,
            valid_until: None,
            notes: None,
            lines: vec![],
        };
        assert!(neither.validate_domain().is_err());

        let both = NewQuote {
            customer_id: Some("c1".into()),
            prospect: Some(Prospect {
                name: "Acme".into(),
                email: None,
                phone: None,
            }),
            currency: None,
            valid_until: None,
            notes: None,
            lines: vec![],
        };
        assert!(both.validate_domain().is_err());
    }

    #[test]
    fn manual_line_totals_qty_times_unit() {
        let line = QuoteLine::Manual {
            line_id: "l".into(),
            description: "Delivery".into(),
            qty: 2,
            unit_minor: 18_500,
        };
        assert_eq!(line.total_minor(), 37_000);
    }
}
