//! SurrealDB-backed quote documents (Step 3, `docs/staff-quoting.md`).
//!
//! Embedded, counter-numbered, tenant-scoped — the `order` conventions. Engine
//! lines are priced by the API layer; the repo stores the resulting
//! [`QuoteLine`]s verbatim as a JSON array (`serde_json::Value: SurrealValue`,
//! the same bridge the pricing catalog uses). Conversion reuses
//! [`SurrealOrderRepo`] so a converted quote goes through the same
//! customer-active / lead-promotion path as a hand-entered order.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, Utc};
use domain::error::{ConflictReason, DomainError};
use domain::money::Money;
use domain::order::{LineItem, NewOrder, Order, OrderRepo};
use domain::quote::{
    Prospect, Quote, QuoteLine, QuoteListQuery, QuoteRepo, QuoteStatus, QuoteWrite,
    quote_total_minor, split_residual_minor,
};
use domain::tenant::Tenant;
use serde_json::Value;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, SurrealValue};
use ulid::Ulid;

use crate::common::{
    CountRow, IdOnly, SearchHitRow, map_err, non_empty_q, record_key, sort_clause, to_hit,
};
use crate::counter::next_number;
use crate::order_repo::SurrealOrderRepo;
use crate::status::quote_status_from_db;

pub(crate) const TABLE: &str = "quote";

const ALLOWED_SORT_FIELDS: &[&str] = &[
    "number",
    "customer_id",
    "status",
    "currency",
    "created_at",
    "updated_at",
];

const SEARCH_CONDITION: &str = "(number @0@ $q OR notes @1@ $q)";
const SEARCH_SCORE: &str = "(search::score(0) + search::score(1))";

// `customer_id` is optional; guard the dereference so a prospect quote
// (customer_id NONE) projects NONE rather than erroring.
const CUSTOMER_NAME_PROJECTION: &str =
    "(IF customer_id THEN type::record('customer', customer_id).name END) AS customer_name";

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct QuoteRow {
    id: RecordId,
    number: String,
    customer_id: Option<String>,
    customer_name: Option<String>,
    prospect: Option<Value>,
    currency: String,
    status: i64,
    valid_until: Option<String>,
    lines: Value,
    pricelist_version: Option<i64>,
    notes: Option<String>,
    created_by: String,
    revises: Option<String>,
    order_id: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct QuoteContent {
    number: String,
    customer_id: Option<String>,
    prospect: Option<Value>,
    currency: String,
    status: i64,
    valid_until: Option<String>,
    lines: Value,
    pricelist_version: Option<i64>,
    notes: Option<String>,
    created_by: String,
    revises: Option<String>,
    order_id: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct StatusPatch {
    status: i64,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct OrderLinkPatch {
    order_id: String,
    updated_at: String,
}

impl QuoteRow {
    pub(crate) fn key(&self) -> String {
        record_key(&self.id)
    }
}

fn decode_lines(value: Value) -> Result<Vec<QuoteLine>, DomainError> {
    serde_json::from_value(value)
        .map_err(|e| DomainError::Store(format!("quote lines decode: {e}")))
}

fn encode_lines(lines: &[QuoteLine]) -> Result<Value, DomainError> {
    serde_json::to_value(lines).map_err(|e| DomainError::Store(format!("quote lines encode: {e}")))
}

fn decode_prospect(value: Option<Value>) -> Result<Option<Prospect>, DomainError> {
    value
        .map(|v| {
            serde_json::from_value(v)
                .map_err(|e| DomainError::Store(format!("quote prospect decode: {e}")))
        })
        .transpose()
}

impl TryFrom<QuoteRow> for Quote {
    type Error = DomainError;

    fn try_from(row: QuoteRow) -> Result<Self, DomainError> {
        let lines = decode_lines(row.lines)?;
        let total_minor = quote_total_minor(&lines);
        Ok(Quote {
            id: record_key(&row.id),
            number: row.number,
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            prospect: decode_prospect(row.prospect)?,
            currency: row.currency,
            status: quote_status_from_db(row.status)?,
            valid_until: row.valid_until,
            lines,
            pricelist_version: row.pricelist_version,
            notes: row.notes,
            created_by: row.created_by,
            revises: row.revises,
            order_id: row.order_id,
            total_minor,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

/// Whether a quote with `valid_until` has expired as of `now`. Accepts an
/// RFC3339 instant or a bare `YYYY-MM-DD` date (the UI's date input); a date is
/// valid through the end of that day. Unparseable or absent → not expired.
fn is_expired(valid_until: Option<&str>, now: DateTime<Utc>) -> bool {
    let Some(raw) = valid_until else {
        return false;
    };
    if let Ok(instant) = DateTime::parse_from_rfc3339(raw) {
        return now > instant.with_timezone(&Utc);
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return now.date_naive() > date;
    }
    false
}

fn where_clause(query: &QuoteListQuery) -> String {
    let mut conditions = Vec::new();
    if query.customer_id.is_some() {
        conditions.push("customer_id = $customer_id");
    }
    if query.status.is_some() {
        conditions.push("status = $status");
    }
    if non_empty_q(&query.q).is_some() {
        conditions.push(SEARCH_CONDITION);
    }
    if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    }
}

pub struct SurrealQuoteRepo {
    session: Arc<Surreal<Any>>,
}

impl SurrealQuoteRepo {
    pub fn new(session: Arc<Surreal<Any>>) -> Self {
        Self { session }
    }

    fn content_for(
        &self,
        number: String,
        status: QuoteStatus,
        data: &QuoteWrite,
        revises: Option<String>,
        order_id: Option<String>,
        created_at: String,
    ) -> Result<QuoteContent, DomainError> {
        Ok(QuoteContent {
            number,
            customer_id: data.customer_id.clone(),
            prospect: data
                .prospect
                .as_ref()
                .map(serde_json::to_value)
                .transpose()
                .map_err(|e| DomainError::Store(format!("quote prospect encode: {e}")))?,
            currency: data.currency.clone(),
            status: status.code() as i64,
            valid_until: data.valid_until.clone(),
            lines: encode_lines(&data.lines)?,
            pricelist_version: data.pricelist_version,
            notes: data.notes.clone(),
            created_by: data.created_by.clone(),
            revises,
            order_id,
            created_at,
            updated_at: Utc::now().to_rfc3339(),
        })
    }

    async fn store(
        &self,
        id: &str,
        content: QuoteContent,
        create: bool,
    ) -> Result<(), DomainError> {
        let row: Option<IdOnly> = if create {
            self.session
                .create((TABLE, id))
                .content(content)
                .await
                .map_err(map_err)?
        } else {
            self.session
                .update((TABLE, id))
                .content(content)
                .await
                .map_err(map_err)?
        };
        row.map(|_| ()).ok_or(DomainError::NotFound)
    }
}

#[async_trait]
impl QuoteRepo for SurrealQuoteRepo {
    async fn list(&self, query: QuoteListQuery) -> Result<domain::Paged<Quote>, DomainError> {
        let q = non_empty_q(&query.q);
        let start = (query.page.saturating_sub(1) as i64) * query.limit as i64;
        let filters = where_clause(&query);

        let mut list_query = if q.is_some() {
            self.session.query(format!(
                "SELECT *, {CUSTOMER_NAME_PROJECTION}, {SEARCH_SCORE} AS score FROM type::table($table) {filters} ORDER BY score DESC LIMIT $limit START $start"
            ))
        } else {
            let order = sort_clause(&query.sort, ALLOWED_SORT_FIELDS)?;
            self.session.query(format!(
                "SELECT *, {CUSTOMER_NAME_PROJECTION} FROM type::table($table) {filters} ORDER BY {order} LIMIT $limit START $start"
            ))
        }
        .bind(("table", TABLE))
        .bind(("limit", query.limit as i64))
        .bind(("start", start));
        if let Some(customer_id) = &query.customer_id {
            list_query = list_query.bind(("customer_id", customer_id.clone()));
        }
        if let Some(status) = query.status {
            list_query = list_query.bind(("status", status.code() as i64));
        }
        if let Some(q) = q {
            list_query = list_query.bind(("q", q.to_string()));
        }
        let mut response = list_query.await.map_err(map_err)?;
        let rows: Vec<QuoteRow> = response.take(0).map_err(map_err)?;

        let mut count_query = if q.is_some() {
            self.session.query(format!(
                "SELECT count() FROM (SELECT id FROM type::table($table) {filters}) GROUP ALL"
            ))
        } else {
            self.session.query(format!(
                "SELECT count() FROM type::table($table) {filters} GROUP ALL"
            ))
        }
        .bind(("table", TABLE));
        if let Some(customer_id) = &query.customer_id {
            count_query = count_query.bind(("customer_id", customer_id.clone()));
        }
        if let Some(status) = query.status {
            count_query = count_query.bind(("status", status.code() as i64));
        }
        if let Some(q) = q {
            count_query = count_query.bind(("q", q.to_string()));
        }
        let mut count_response = count_query.await.map_err(map_err)?;
        let count_rows: Vec<CountRow> = count_response.take(0).map_err(map_err)?;
        let total = count_rows.first().map(|r| r.count as u64).unwrap_or(0);

        let items = rows
            .into_iter()
            .map(Quote::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(domain::Paged {
            items,
            total,
            page: query.page,
            limit: query.limit,
        })
    }

    async fn get(&self, id: &str) -> Result<Option<Quote>, DomainError> {
        let mut response = self
            .session
            .query(format!(
                "SELECT *, {CUSTOMER_NAME_PROJECTION} FROM type::record($table, $id)"
            ))
            .bind(("table", TABLE))
            .bind(("id", id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<QuoteRow> = response.take(0).map_err(map_err)?;
        rows.into_iter().next().map(Quote::try_from).transpose()
    }

    async fn create(&self, data: QuoteWrite, tenant: &Tenant) -> Result<Quote, DomainError> {
        let number = next_number(&self.session, "quote", &tenant.quote_prefix).await?;
        let id = Ulid::new().to_string();
        let content = self.content_for(
            number,
            QuoteStatus::Draft,
            &data,
            None,
            None,
            Utc::now().to_rfc3339(),
        )?;
        self.store(&id, content, true).await?;
        self.get(&id)
            .await?
            .ok_or_else(|| DomainError::Store("quote create returned no row".to_string()))
    }

    async fn update(&self, id: &str, mut data: QuoteWrite) -> Result<Quote, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        if !domain::quote::can_edit(existing.status) {
            return Err(DomainError::Conflict(ConflictReason::QuoteNotDraft));
        }
        // `created_by` is set once, at creation — never reassigned on edit.
        data.created_by = existing.created_by;
        let content = self.content_for(
            existing.number,
            existing.status,
            &data,
            existing.revises,
            existing.order_id,
            existing.created_at,
        )?;
        self.store(id, content, false).await?;
        self.get(id).await?.ok_or(DomainError::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        if !domain::quote::can_edit(existing.status) {
            return Err(DomainError::Conflict(ConflictReason::QuoteNotDraft));
        }
        let row: Option<IdOnly> = self.session.delete((TABLE, id)).await.map_err(map_err)?;
        row.map(|_| ()).ok_or(DomainError::NotFound)
    }

    async fn set_status(&self, id: &str, status: QuoteStatus) -> Result<Quote, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        domain::quote::validate_transition(existing.status, status)?;

        // Accepting an already-overdue quote expires it instead (the
        // acceptance-time expiry re-check, `docs/staff-quoting.md` lifecycle).
        let effective = if status == QuoteStatus::Accepted
            && is_expired(existing.valid_until.as_deref(), Utc::now())
        {
            QuoteStatus::Expired
        } else {
            status
        };

        let patch = StatusPatch {
            status: effective.code() as i64,
            updated_at: Utc::now().to_rfc3339(),
        };
        let row: Option<IdOnly> = self
            .session
            .update((TABLE, id))
            .merge(patch)
            .await
            .map_err(map_err)?;
        row.ok_or(DomainError::NotFound)?;

        if effective == QuoteStatus::Expired && status == QuoteStatus::Accepted {
            return Err(DomainError::Conflict(ConflictReason::QuoteExpired));
        }
        self.get(id).await?.ok_or(DomainError::NotFound)
    }

    async fn clone_quote(
        &self,
        id: &str,
        tenant: &Tenant,
        created_by: &str,
    ) -> Result<Quote, DomainError> {
        let source = self.get(id).await?.ok_or(DomainError::NotFound)?;
        let data = QuoteWrite {
            customer_id: source.customer_id,
            prospect: source.prospect,
            currency: source.currency,
            valid_until: source.valid_until,
            notes: source.notes,
            lines: source.lines,
            pricelist_version: source.pricelist_version,
            created_by: created_by.to_string(),
        };
        let number = next_number(&self.session, "quote", &tenant.quote_prefix).await?;
        let new_id = Ulid::new().to_string();
        let content = self.content_for(
            number,
            QuoteStatus::Draft,
            &data,
            Some(source.id),
            None,
            Utc::now().to_rfc3339(),
        )?;
        self.store(&new_id, content, true).await?;
        self.get(&new_id)
            .await?
            .ok_or_else(|| DomainError::Store("quote clone returned no row".to_string()))
    }

    async fn convert_to_order(
        &self,
        id: &str,
        tenant: &Tenant,
        now: DateTime<Utc>,
    ) -> Result<Order, DomainError> {
        let quote = self.get(id).await?.ok_or(DomainError::NotFound)?;
        if quote.status != QuoteStatus::Accepted {
            return Err(DomainError::Conflict(ConflictReason::QuoteNotAccepted));
        }
        if quote.order_id.is_some() {
            return Err(DomainError::Conflict(ConflictReason::QuoteAlreadyConverted));
        }
        let Some(customer_id) = quote.customer_id.clone() else {
            return Err(DomainError::Conflict(
                ConflictReason::QuoteConvertRequiresCustomer,
            ));
        };
        if is_expired(quote.valid_until.as_deref(), now) {
            let patch = StatusPatch {
                status: QuoteStatus::Expired.code() as i64,
                updated_at: now.to_rfc3339(),
            };
            let _: Option<IdOnly> = self
                .session
                .update((TABLE, id))
                .merge(patch)
                .await
                .map_err(map_err)?;
            return Err(DomainError::Conflict(ConflictReason::QuoteExpired));
        }

        let money = |amount_minor: i64| Money {
            amount_minor,
            currency: quote.currency.clone(),
        };
        let mut line_items = Vec::new();
        for line in &quote.lines {
            match line {
                QuoteLine::Manual {
                    description,
                    qty,
                    unit_minor,
                    ..
                } => line_items.push(LineItem {
                    description: description.clone(),
                    quantity: *qty,
                    unit_price: money(*unit_minor),
                }),
                QuoteLine::Spec {
                    description,
                    qty,
                    pricing,
                    line_id,
                    ..
                }
                | QuoteLine::Template {
                    template: description,
                    qty,
                    pricing,
                    line_id,
                    ..
                } => {
                    for split in
                        split_residual_minor(description, *qty, pricing.final_total_minor, line_id)
                    {
                        line_items.push(LineItem {
                            description: split.description,
                            quantity: split.qty,
                            unit_price: money(split.unit_minor),
                        });
                    }
                }
            }
        }

        // Guard the price-preservation invariant before creating the order.
        let items_total: i64 = line_items
            .iter()
            .map(|item| item.unit_price.amount_minor * item.quantity as i64)
            .sum();
        if items_total != quote.total_minor {
            return Err(DomainError::Store(
                "quote→order conversion changed the total".to_string(),
            ));
        }

        let order_repo = SurrealOrderRepo::new(self.session.clone());
        let order = order_repo
            .create(
                NewOrder {
                    customer_id,
                    currency: Some(quote.currency.clone()),
                    line_items,
                    notes: quote.notes.clone(),
                },
                tenant,
            )
            .await?;

        let patch = OrderLinkPatch {
            order_id: order.id.clone(),
            updated_at: now.to_rfc3339(),
        };
        let _: Option<IdOnly> = self
            .session
            .update((TABLE, id))
            .merge(patch)
            .await
            .map_err(map_err)?;

        Ok(order)
    }

    async fn search(&self, q: &str, limit: u32) -> Result<Vec<domain::SearchHit>, DomainError> {
        let mut response = self
            .session
            .query(format!(
                "SELECT id, number AS label, search::highlight('<b>', '</b>', 0) AS highlight, {SEARCH_SCORE} AS score \
                 FROM type::table($table) WHERE {SEARCH_CONDITION} ORDER BY score DESC LIMIT $limit"
            ))
            .bind(("table", TABLE))
            .bind(("q", q.to_string()))
            .bind(("limit", limit as i64))
            .await
            .map_err(map_err)?;
        let rows: Vec<SearchHitRow> = response.take(0).map_err(map_err)?;
        Ok(rows.into_iter().map(to_hit).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 22, 12, 0, 0).unwrap()
    }

    #[test]
    fn date_valid_until_expires_the_day_after() {
        assert!(!is_expired(Some("2026-07-22"), now()));
        assert!(!is_expired(Some("2026-07-23"), now()));
        assert!(is_expired(Some("2026-07-21"), now()));
    }

    #[test]
    fn absent_or_unparseable_valid_until_never_expires() {
        assert!(!is_expired(None, now()));
        assert!(!is_expired(Some("garbage"), now()));
    }

    #[test]
    fn rfc3339_valid_until_compares_instants() {
        assert!(is_expired(Some("2026-07-22T11:00:00+00:00"), now()));
        assert!(!is_expired(Some("2026-07-22T13:00:00+00:00"), now()));
    }
}
