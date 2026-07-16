use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use domain::Paged;
use domain::customer::{CustomerStatus, can_order};
use domain::error::{ConflictReason, DomainError, FieldError};
use domain::money::Money;
use domain::order::{
    CustomerActivity, LineItem, MonthlyOrderCount, NewOrder, Order, OrderListQuery, OrderRepo,
    OrderStatus, StatusCount, line_items_total, validate_transition,
};
use domain::tenant::Tenant;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use ulid::Ulid;

use crate::counter::next_number;
use crate::status::{customer_status_from_db, order_status_from_db};

pub(crate) const TABLE: &str = "order";
const CUSTOMER_TABLE: &str = "customer";
const INVOICE_TABLE: &str = "invoice";

// Whitelisted, not bound as a query parameter — see customer_repo's
// ALLOWED_SORT_FIELDS for why.
const ALLOWED_SORT_FIELDS: &[&str] = &[
    "number",
    "customer_id",
    "status",
    "currency",
    "created_at",
    "updated_at",
];

// Shared with `invoice_repo`, which has its own money/line-item fields
// (net/tax/gross totals, invoice line items copied from the order).
#[derive(Debug, Clone, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct MoneyRow {
    amount_minor: i64,
    currency: String,
}

impl From<Money> for MoneyRow {
    fn from(m: Money) -> Self {
        MoneyRow {
            amount_minor: m.amount_minor,
            currency: m.currency,
        }
    }
}

impl From<MoneyRow> for Money {
    fn from(m: MoneyRow) -> Self {
        Money {
            amount_minor: m.amount_minor,
            currency: m.currency,
        }
    }
}

#[derive(Debug, Clone, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct LineItemRow {
    description: String,
    quantity: u32,
    unit_price: MoneyRow,
}

impl From<LineItem> for LineItemRow {
    fn from(item: LineItem) -> Self {
        LineItemRow {
            description: item.description,
            quantity: item.quantity,
            unit_price: item.unit_price.into(),
        }
    }
}

impl From<LineItemRow> for LineItem {
    fn from(row: LineItemRow) -> Self {
        LineItem {
            description: row.description,
            quantity: row.quantity,
            unit_price: row.unit_price.into(),
        }
    }
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct OrderRow {
    id: RecordId,
    number: String,
    customer_id: String,
    customer_name: Option<String>,
    status: i64,
    currency: String,
    line_items: Vec<LineItemRow>,
    total: MoneyRow,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct OrderContent {
    number: String,
    customer_id: String,
    status: i64,
    currency: String,
    line_items: Vec<LineItemRow>,
    total: MoneyRow,
    notes: Option<String>,
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
struct IdOnly {
    id: RecordId,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CustomerStatusRow {
    status: i64,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CountRow {
    count: i64,
}

// Slim per-order projection for `customer_activity` — no line items, no
// customer-name join. Aggregation happens in Rust (`aggregate_activity`)
// rather than SurrealQL GROUP BY: it keeps the count/sum/date-window logic
// unit-testable and sidesteps the fulltext-era count() mis-planning quirk
// noted in `list` (see docs/adr/0001-surrealdb-fulltext-keyword.md).
#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct ActivityRow {
    status: i64,
    amount: i64,
    currency: String,
    created_at: String,
}

const MONTHS_IN_SPARKLINE: usize = 12;
const ACTIVITY_WINDOW_DAYS: i64 = 30;

fn month_key(year: i32, month: u32) -> String {
    format!("{year:04}-{month:02}")
}

/// The last `MONTHS_IN_SPARKLINE` `YYYY-MM` keys up to and including `now`'s
/// month, oldest first.
fn recent_month_keys(now: DateTime<Utc>) -> Vec<String> {
    let (mut year, mut month) = (now.year(), now.month());
    let mut keys = Vec::with_capacity(MONTHS_IN_SPARKLINE);
    for _ in 0..MONTHS_IN_SPARKLINE {
        keys.push(month_key(year, month));
        if month == 1 {
            month = 12;
            year -= 1;
        } else {
            month -= 1;
        }
    }
    keys.reverse();
    keys
}

/// Aggregates a customer's orders into the detail-page activity summary.
/// Pure over `(rows, now)` so it can be exercised without a database.
/// `created_at` values that don't parse as RFC3339 still count toward totals
/// and status breakdown but are excluded from `last_order_at` and the
/// date-windowed figures. Timestamps after `now` are also excluded from the
/// 30-day window and monthly sparkline.
fn aggregate_activity(rows: Vec<ActivityRow>, now: DateTime<Utc>) -> CustomerActivity {
    let window_start = now - chrono::Duration::days(ACTIVITY_WINDOW_DAYS);

    let mut status_counts: HashMap<u8, u64> = HashMap::new();
    let mut month_counts: HashMap<String, u64> = HashMap::new();
    let mut spend_minor: i64 = 0;
    let mut spend_currency: Option<String> = None;
    let mut any_currency: Option<String> = None;
    let mut last_order_at: Option<String> = None;
    let mut last_order_dt: Option<DateTime<Utc>> = None;
    let mut orders_last_30_days: u64 = 0;

    for row in &rows {
        if let Ok(code) = u8::try_from(row.status) {
            *status_counts.entry(code).or_insert(0) += 1;
        }
        any_currency.get_or_insert_with(|| row.currency.clone());
        if row.status == OrderStatus::Completed.code() as i64 {
            spend_minor += row.amount;
            spend_currency.get_or_insert_with(|| row.currency.clone());
        }
        if let Ok(parsed) = DateTime::parse_from_rfc3339(&row.created_at) {
            let parsed = parsed.with_timezone(&Utc);
            if last_order_dt.is_none_or(|max| parsed > max) {
                last_order_dt = Some(parsed);
                last_order_at = Some(row.created_at.clone());
            }
            if parsed <= now {
                if parsed >= window_start {
                    orders_last_30_days += 1;
                }
                *month_counts
                    .entry(month_key(parsed.year(), parsed.month()))
                    .or_insert(0) += 1;
            }
        }
    }

    let mut status_counts: Vec<StatusCount> = status_counts
        .into_iter()
        .filter_map(|(code, count)| OrderStatus::from_code(code).map(|status| StatusCount { status, count }))
        .collect();
    status_counts.sort_by_key(|entry| entry.status.code());

    let orders_by_month = recent_month_keys(now)
        .into_iter()
        .map(|month| MonthlyOrderCount {
            count: month_counts.get(&month).copied().unwrap_or(0),
            month,
        })
        .collect();

    let currency = spend_currency
        .or(any_currency)
        .unwrap_or_else(|| "EUR".to_string());

    CustomerActivity {
        total_orders: rows.len() as u64,
        status_counts,
        total_spend: Money {
            amount_minor: spend_minor,
            currency,
        },
        last_order_at,
        orders_last_30_days,
        orders_by_month,
    }
}

fn record_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(key) => key.clone(),
        other => format!("{other:?}"),
    }
}

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

fn customer_not_found_error() -> DomainError {
    let mut details = HashMap::new();
    details.insert("customer_id".to_string(), FieldError::code("not_found"));
    DomainError::Validation(details)
}

impl OrderRow {
    pub(crate) fn key(&self) -> String {
        record_key(&self.id)
    }
}

impl TryFrom<OrderRow> for Order {
    type Error = DomainError;

    fn try_from(row: OrderRow) -> Result<Self, DomainError> {
        Ok(Order {
            id: record_key(&row.id),
            number: row.number,
            customer_id: row.customer_id,
            customer_name: row.customer_name,
            status: order_status_from_db(row.status)?,
            currency: row.currency,
            line_items: row.line_items.into_iter().map(LineItem::from).collect(),
            total: row.total.into(),
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        })
    }
}

fn sort_clause(sort: &str) -> Result<String, DomainError> {
    let (field, dir) = match sort.strip_prefix('-') {
        Some(field) => (field, "DESC"),
        None => (sort, "ASC"),
    };
    if !ALLOWED_SORT_FIELDS.contains(&field) {
        let mut details = HashMap::new();
        details.insert(
            "sort".to_string(),
            FieldError::with_params(
                "unknown_sort_field",
                HashMap::from([("field".to_string(), field.to_string())]),
            ),
        );
        return Err(DomainError::Validation(details));
    }
    Ok(format!("{field} {dir}"))
}

fn non_empty_q(q: &Option<String>) -> Option<&str> {
    q.as_deref().filter(|s| !s.is_empty())
}

// One FULLTEXT index per field (see migrations/0004_search.surql) means
// each predicate needs its own match reference — reusing one across fields
// errors with "Duplicated Match reference" on this SurrealDB version. See
// docs/adr/0001-surrealdb-fulltext-keyword.md.
// line_items[*].description is deliberately excluded: SurrealDB 3.2's
// FULLTEXT index on an array field can't push the LIMIT into the index scan
// (EXPLAIN shows "Iterate Index" + "MemoryOrderedLimit" instead of the
// scalar-field "FullTextScan" with a pushed limit), so it collects every
// match before ranking — with the seeded data's ~10-value line-item
// vocabulary, a common 3-letter prefix matches tens of thousands of rows and
// costs 1-2s. Revisit once the order/line-item entity structure is final;
// see docs/adr/0003-order-search-excludes-line-items.md.
const SEARCH_CONDITION: &str = "(number @0@ $q OR notes @1@ $q)";
const SEARCH_SCORE: &str = "(search::score(0) + search::score(1))";

// `customer_id` is a plain string key, not a record link, so it must be
// re-formed into a record id before the name can be dereferenced.
const CUSTOMER_NAME_PROJECTION: &str =
    "type::record('customer', customer_id).name AS customer_name";

/// Builds the `WHERE` clause shared by the list and count queries. Returns
/// an empty string when no filters apply.
fn where_clause(query: &OrderListQuery) -> String {
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

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct SearchHitRow {
    id: RecordId,
    label: String,
    highlight: Option<String>,
}

fn to_hit(row: SearchHitRow) -> domain::SearchHit {
    domain::SearchHit {
        id: record_key(&row.id),
        highlight: row.highlight.unwrap_or_else(|| row.label.clone()),
        label: row.label,
    }
}

pub struct SurrealOrderRepo {
    session: Arc<Surreal<Any>>,
}

impl SurrealOrderRepo {
    pub fn new(session: Arc<Surreal<Any>>) -> Self {
        Self { session }
    }

    async fn customer_exists(&self, customer_id: &str) -> Result<bool, DomainError> {
        let row: Option<IdOnly> = self
            .session
            .select((CUSTOMER_TABLE, customer_id))
            .await
            .map_err(map_err)?;
        Ok(row.is_some())
    }

    async fn customer_status(
        &self,
        customer_id: &str,
    ) -> Result<Option<CustomerStatus>, DomainError> {
        let row: Option<CustomerStatusRow> = self
            .session
            .select((CUSTOMER_TABLE, customer_id))
            .await
            .map_err(map_err)?;
        row.map(|r| customer_status_from_db(r.status)).transpose()
    }

    /// A `lead` placing its first order *is* the conversion event — promote
    /// it to `active` in the same operation as order creation (see
    /// `docs/customers-crm.md`). The write goes through this session so the
    /// existing `LIVE SELECT` on `customer` picks it up like any other
    /// update.
    async fn promote_customer_to_active(&self, customer_id: &str) -> Result<(), DomainError> {
        let patch = StatusPatch {
            status: CustomerStatus::Active.code() as i64,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        let _: Option<IdOnly> = self
            .session
            .update((CUSTOMER_TABLE, customer_id))
            .merge(patch)
            .await
            .map_err(map_err)?;
        Ok(())
    }

    async fn has_invoice(&self, order_id: &str) -> Result<bool, DomainError> {
        let mut response = self
            .session
            .query("SELECT id FROM type::table($table) WHERE order_id = $order_id LIMIT 1")
            .bind(("table", INVOICE_TABLE))
            .bind(("order_id", order_id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<IdOnly> = response.take(0).map_err(map_err)?;
        Ok(!rows.is_empty())
    }
}

#[async_trait]
impl OrderRepo for SurrealOrderRepo {
    async fn list(&self, query: OrderListQuery) -> Result<Paged<Order>, DomainError> {
        let q = non_empty_q(&query.q);
        let start = (query.page.saturating_sub(1) as i64) * query.limit as i64;
        let filters = where_clause(&query);

        let mut list_query = if q.is_some() {
            self.session.query(format!(
                "SELECT *, {CUSTOMER_NAME_PROJECTION}, {SEARCH_SCORE} AS score FROM type::table($table) {filters} ORDER BY score DESC LIMIT $limit START $start"
            ))
        } else {
            let order = sort_clause(&query.sort)?;
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
        let rows: Vec<OrderRow> = response.take(0).map_err(map_err)?;

        // A bare `SELECT count() ... WHERE <fulltext predicate> GROUP ALL`
        // mis-plans to 0 on this SurrealDB version; wrap the same filters in
        // a subquery instead. See docs/adr/0001-surrealdb-fulltext-keyword.md.
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
            .map(Order::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Paged {
            items,
            total,
            page: query.page,
            limit: query.limit,
        })
    }

    async fn get(&self, id: &str) -> Result<Option<Order>, DomainError> {
        let mut response = self
            .session
            .query(format!(
                "SELECT *, {CUSTOMER_NAME_PROJECTION} FROM type::record($table, $id)"
            ))
            .bind(("table", TABLE))
            .bind(("id", id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<OrderRow> = response.take(0).map_err(map_err)?;
        rows.into_iter().next().map(Order::try_from).transpose()
    }

    async fn customer_activity(
        &self,
        customer_id: &str,
        now: DateTime<Utc>,
    ) -> Result<CustomerActivity, DomainError> {
        let mut response = self
            .session
            .query(
                "SELECT status, total.amount_minor AS amount, currency, created_at \
                 FROM type::table($table) WHERE customer_id = $customer_id",
            )
            .bind(("table", TABLE))
            .bind(("customer_id", customer_id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<ActivityRow> = response.take(0).map_err(map_err)?;
        Ok(aggregate_activity(rows, now))
    }

    async fn create(&self, data: NewOrder, tenant: &Tenant) -> Result<Order, DomainError> {
        let customer_status = self
            .customer_status(&data.customer_id)
            .await?
            .ok_or_else(customer_not_found_error)?;
        if !can_order(customer_status) {
            return Err(DomainError::Conflict(
                ConflictReason::CustomerNotActiveForOrder,
            ));
        }
        if customer_status == CustomerStatus::Lead {
            self.promote_customer_to_active(&data.customer_id).await?;
        }
        let currency = data.currency.clone().unwrap_or_else(|| "EUR".to_string());
        let total = line_items_total(&data.line_items, &currency);
        let number = next_number(&self.session, "order", &tenant.order_prefix).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        let content = OrderContent {
            number,
            customer_id: data.customer_id,
            status: OrderStatus::Draft.code() as i64,
            currency,
            line_items: data.line_items.into_iter().map(LineItemRow::from).collect(),
            total: total.into(),
            notes: data.notes,
            created_at: now.clone(),
            updated_at: now,
        };

        // Mutations return the stored row, which lacks the read-time
        // customer_name join — re-fetch through `get` for the full shape.
        let row: Option<IdOnly> = self
            .session
            .create((TABLE, id.clone()))
            .content(content)
            .await
            .map_err(map_err)?;
        row.ok_or_else(|| DomainError::Store("order create returned no row".to_string()))?;

        self.get(&id)
            .await?
            .ok_or_else(|| DomainError::Store("order create returned no row".to_string()))
    }

    async fn update(&self, id: &str, data: NewOrder) -> Result<Order, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        if !self.customer_exists(&data.customer_id).await? {
            return Err(customer_not_found_error());
        }
        let currency = data.currency.clone().unwrap_or_else(|| "EUR".to_string());
        let total = line_items_total(&data.line_items, &currency);
        let now = chrono::Utc::now().to_rfc3339();
        let content = OrderContent {
            number: existing.number,
            customer_id: data.customer_id,
            status: existing.status.code() as i64,
            currency,
            line_items: data.line_items.into_iter().map(LineItemRow::from).collect(),
            total: total.into(),
            notes: data.notes,
            created_at: existing.created_at,
            updated_at: now,
        };

        let row: Option<IdOnly> = self
            .session
            .update((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;
        row.ok_or(DomainError::NotFound)?;

        self.get(id).await?.ok_or(DomainError::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        if self.has_invoice(id).await? {
            return Err(DomainError::Conflict(ConflictReason::OrderHasInvoice));
        }
        let row: Option<IdOnly> = self.session.delete((TABLE, id)).await.map_err(map_err)?;
        row.map(|_| ()).ok_or(DomainError::NotFound)
    }

    async fn set_status(&self, id: &str, status: OrderStatus) -> Result<Order, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        validate_transition(existing.status, status)?;

        let patch = StatusPatch {
            status: status.code() as i64,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        let row: Option<IdOnly> = self
            .session
            .update((TABLE, id))
            .merge(patch)
            .await
            .map_err(map_err)?;
        row.ok_or(DomainError::NotFound)?;

        self.get(id).await?.ok_or(DomainError::NotFound)
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
mod activity_tests {
    use super::*;
    use chrono::TimeZone;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 16, 12, 0, 0).unwrap()
    }

    fn row(status: OrderStatus, amount: i64, created_at: &str) -> ActivityRow {
        ActivityRow {
            status: status.code() as i64,
            amount,
            currency: "EUR".to_string(),
            created_at: created_at.to_string(),
        }
    }

    fn count_for(activity: &CustomerActivity, status: OrderStatus) -> u64 {
        activity
            .status_counts
            .iter()
            .find(|c| c.status == status)
            .map_or(0, |c| c.count)
    }

    #[test]
    fn empty_customer_has_zeroed_activity() {
        let activity = aggregate_activity(vec![], now());
        assert_eq!(activity.total_orders, 0);
        assert!(activity.status_counts.is_empty());
        assert_eq!(activity.total_spend.amount_minor, 0);
        assert_eq!(activity.total_spend.currency, "EUR");
        assert_eq!(activity.last_order_at, None);
        assert_eq!(activity.orders_last_30_days, 0);
        assert_eq!(activity.orders_by_month.len(), MONTHS_IN_SPARKLINE);
        assert!(activity.orders_by_month.iter().all(|m| m.count == 0));
    }

    #[test]
    fn aggregates_counts_spend_recency_and_months() {
        let rows = vec![
            row(OrderStatus::Completed, 1000, "2026-07-10T09:00:00+00:00"),
            row(OrderStatus::Completed, 500, "2026-06-01T09:00:00+00:00"),
            row(OrderStatus::Draft, 999, "2026-07-15T09:00:00+00:00"),
            row(OrderStatus::Cancelled, 200, "2025-08-01T09:00:00+00:00"),
        ];
        let activity = aggregate_activity(rows, now());

        assert_eq!(activity.total_orders, 4);
        assert_eq!(count_for(&activity, OrderStatus::Completed), 2);
        assert_eq!(count_for(&activity, OrderStatus::Draft), 1);
        assert_eq!(count_for(&activity, OrderStatus::Cancelled), 1);

        // Spend counts completed orders only; the draft's amount is excluded.
        assert_eq!(activity.total_spend.amount_minor, 1500);

        assert_eq!(
            activity.last_order_at.as_deref(),
            Some("2026-07-15T09:00:00+00:00")
        );

        // Window start is 2026-06-16, so only the two July orders qualify.
        assert_eq!(activity.orders_last_30_days, 2);

        let month = |key: &str| {
            activity
                .orders_by_month
                .iter()
                .find(|m| m.month == key)
                .map_or(0, |m| m.count)
        };
        assert_eq!(month("2026-07"), 2);
        assert_eq!(month("2026-06"), 1);
        assert_eq!(month("2025-08"), 1);
    }

    #[test]
    fn status_counts_are_ordered_by_status_code() {
        let rows = vec![
            row(OrderStatus::Cancelled, 0, "2026-07-01T09:00:00+00:00"),
            row(OrderStatus::Draft, 0, "2026-07-01T09:00:00+00:00"),
            row(OrderStatus::Completed, 0, "2026-07-01T09:00:00+00:00"),
        ];
        let activity = aggregate_activity(rows, now());
        let codes: Vec<u8> = activity
            .status_counts
            .iter()
            .map(|c| c.status.code())
            .collect();
        assert_eq!(codes, vec![0, 3, 4]);
    }

    #[test]
    fn sparkline_spans_twelve_months_ending_this_month() {
        let keys = recent_month_keys(now());
        assert_eq!(keys.len(), MONTHS_IN_SPARKLINE);
        assert_eq!(keys.first().unwrap(), "2025-08");
        assert_eq!(keys.last().unwrap(), "2026-07");
    }

    #[test]
    fn last_order_uses_parsed_instant_not_string_order() {
        // Lexicographically "…T10:00:00+02:00" > "…T09:00:00+00:00", but the
        // UTC offset means the second timestamp is actually later (09:00Z vs 08:00Z).
        let rows = vec![
            row(OrderStatus::Completed, 100, "2026-07-15T10:00:00+02:00"),
            row(OrderStatus::Completed, 100, "2026-07-15T09:00:00+00:00"),
        ];
        let activity = aggregate_activity(rows, now());
        assert_eq!(
            activity.last_order_at.as_deref(),
            Some("2026-07-15T09:00:00+00:00")
        );
    }

    #[test]
    fn future_timestamps_are_excluded_from_window_and_months() {
        let rows = vec![
            row(OrderStatus::Completed, 100, "2026-07-10T09:00:00+00:00"),
            row(OrderStatus::Draft, 0, "2026-07-20T09:00:00+00:00"),
            row(OrderStatus::Cancelled, 0, "not-a-timestamp"),
        ];
        let activity = aggregate_activity(rows, now());

        assert_eq!(activity.total_orders, 3);
        assert_eq!(activity.orders_last_30_days, 1);
        assert_eq!(
            activity
                .orders_by_month
                .iter()
                .find(|m| m.month == "2026-07")
                .map(|m| m.count),
            Some(1)
        );
        // Latest parseable instant wins; unparseable rows do not.
        assert_eq!(
            activity.last_order_at.as_deref(),
            Some("2026-07-20T09:00:00+00:00")
        );
    }

    #[test]
    fn offset_aware_window_boundary() {
        // now = 2026-07-16T12:00:00Z ⇒ window_start = 2026-06-16T12:00:00Z.
        // Same wall-clock hour, different offsets straddle the boundary.
        let rows = vec![
            row(OrderStatus::Completed, 100, "2026-06-16T14:00:00+02:00"), // 12:00Z — in
            row(OrderStatus::Completed, 100, "2026-06-16T13:00:00+02:00"), // 11:00Z — out
        ];
        let activity = aggregate_activity(rows, now());
        assert_eq!(activity.orders_last_30_days, 1);
    }
}
