use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use domain::Paged;
use domain::error::{ConflictReason, DomainError, FieldError};
use domain::invoice::{
    DEFAULT_TAX_RATE_BP, Invoice, InvoiceListQuery, InvoiceRepo, InvoiceStatus, NewInvoice,
    UpdateInvoice, can_edit, compute_gross, compute_tax, due_date_from_issue, validate_transition,
};
use domain::order::{LineItem, can_invoice, line_items_total, validate_line_item_currencies};
use domain::tenant::Tenant;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use ulid::Ulid;

use crate::counter::next_number;
use crate::exchange_rate::lookup_rate;
use crate::order_repo::{LineItemRow, MoneyRow};
use crate::status::order_status_from_db;

pub(crate) const TABLE: &str = "invoice";
const ORDER_TABLE: &str = "order";

const ALLOWED_SORT_FIELDS: &[&str] = &[
    "number",
    "order_id",
    "customer_id",
    "status",
    "currency",
    "created_at",
    "updated_at",
];

fn status_to_str(status: InvoiceStatus) -> &'static str {
    match status {
        InvoiceStatus::Draft => "draft",
        InvoiceStatus::Issued => "issued",
        InvoiceStatus::Paid => "paid",
        InvoiceStatus::Void => "void",
    }
}

fn status_from_str(value: &str) -> Result<InvoiceStatus, DomainError> {
    match value {
        "draft" => Ok(InvoiceStatus::Draft),
        "issued" => Ok(InvoiceStatus::Issued),
        "paid" => Ok(InvoiceStatus::Paid),
        "void" => Ok(InvoiceStatus::Void),
        other => Err(DomainError::Store(format!(
            "unknown invoice status: {other}"
        ))),
    }
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct OrderSnapshotRow {
    customer_id: String,
    status: i64,
    currency: String,
    line_items: Vec<LineItemRow>,
    total: MoneyRow,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct InvoiceRow {
    id: RecordId,
    number: String,
    order_id: String,
    customer_id: String,
    status: String,
    currency: String,
    exchange_rate: Option<String>,
    line_items: Vec<LineItemRow>,
    net_total: MoneyRow,
    tax_rate_bp: u32,
    tax_total: MoneyRow,
    gross_total: MoneyRow,
    issue_date: Option<String>,
    due_date: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct InvoiceContent {
    number: String,
    order_id: String,
    customer_id: String,
    status: String,
    currency: String,
    exchange_rate: Option<String>,
    line_items: Vec<LineItemRow>,
    net_total: MoneyRow,
    tax_rate_bp: u32,
    tax_total: MoneyRow,
    gross_total: MoneyRow,
    issue_date: Option<String>,
    due_date: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct StatusPatch {
    status: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct IssuePatch {
    status: String,
    issue_date: String,
    due_date: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct IdOnly {
    id: RecordId,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CountRow {
    count: i64,
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

impl InvoiceRow {
    pub(crate) fn key(&self) -> String {
        record_key(&self.id)
    }
}

impl TryFrom<InvoiceRow> for Invoice {
    type Error = DomainError;

    fn try_from(row: InvoiceRow) -> Result<Self, DomainError> {
        Ok(Invoice {
            id: record_key(&row.id),
            number: row.number,
            order_id: row.order_id,
            customer_id: row.customer_id,
            status: status_from_str(&row.status)?,
            currency: row.currency,
            exchange_rate: row.exchange_rate,
            line_items: row.line_items.into_iter().map(LineItem::from).collect(),
            net_total: row.net_total.into(),
            tax_rate_bp: row.tax_rate_bp,
            tax_total: row.tax_total.into(),
            gross_total: row.gross_total.into(),
            issue_date: row.issue_date,
            due_date: row.due_date,
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

// Matches the `invoice_search_number` index (see
// migrations/0004_search.surql).
const SEARCH_CONDITION: &str = "number @0@ $q";
const SEARCH_SCORE: &str = "search::score(0)";

fn where_clause(query: &InvoiceListQuery) -> String {
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

fn order_not_found_error() -> DomainError {
    let mut details = HashMap::new();
    details.insert("order_id".to_string(), FieldError::code("not_found"));
    DomainError::Validation(details)
}

fn currency_mismatch_error(order_currency: &str) -> DomainError {
    let mut details = HashMap::new();
    details.insert(
        "currency".to_string(),
        FieldError::with_params(
            "currency_mismatch",
            HashMap::from([("currency".to_string(), order_currency.to_string())]),
        ),
    );
    DomainError::Validation(details)
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

pub struct SurrealInvoiceRepo {
    session: Arc<Surreal<Any>>,
}

impl SurrealInvoiceRepo {
    pub fn new(session: Arc<Surreal<Any>>) -> Self {
        Self { session }
    }

    async fn invoice_exists_for_order(&self, order_id: &str) -> Result<bool, DomainError> {
        let mut response = self
            .session
            .query("SELECT id FROM type::table($table) WHERE order_id = $order_id LIMIT 1")
            .bind(("table", TABLE))
            .bind(("order_id", order_id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<IdOnly> = response.take(0).map_err(map_err)?;
        Ok(!rows.is_empty())
    }
}

#[async_trait]
impl InvoiceRepo for SurrealInvoiceRepo {
    async fn list(&self, query: InvoiceListQuery) -> Result<Paged<Invoice>, DomainError> {
        let q = non_empty_q(&query.q);
        let start = (query.page.saturating_sub(1) as i64) * query.limit as i64;
        let filters = where_clause(&query);

        let mut list_query = if q.is_some() {
            self.session.query(format!(
                "SELECT *, {SEARCH_SCORE} AS score FROM type::table($table) {filters} ORDER BY score DESC LIMIT $limit START $start"
            ))
        } else {
            let order = sort_clause(&query.sort)?;
            self.session.query(format!(
                "SELECT * FROM type::table($table) {filters} ORDER BY {order} LIMIT $limit START $start"
            ))
        }
        .bind(("table", TABLE))
        .bind(("limit", query.limit as i64))
        .bind(("start", start));
        if let Some(customer_id) = &query.customer_id {
            list_query = list_query.bind(("customer_id", customer_id.clone()));
        }
        if let Some(status) = query.status {
            list_query = list_query.bind(("status", status_to_str(status)));
        }
        if let Some(q) = q {
            list_query = list_query.bind(("q", q.to_string()));
        }
        let mut response = list_query.await.map_err(map_err)?;
        let rows: Vec<InvoiceRow> = response.take(0).map_err(map_err)?;

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
            count_query = count_query.bind(("status", status_to_str(status)));
        }
        if let Some(q) = q {
            count_query = count_query.bind(("q", q.to_string()));
        }
        let mut count_response = count_query.await.map_err(map_err)?;
        let count_rows: Vec<CountRow> = count_response.take(0).map_err(map_err)?;
        let total = count_rows.first().map(|r| r.count as u64).unwrap_or(0);

        let items = rows
            .into_iter()
            .map(Invoice::try_from)
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Paged {
            items,
            total,
            page: query.page,
            limit: query.limit,
        })
    }

    async fn get(&self, id: &str) -> Result<Option<Invoice>, DomainError> {
        let row: Option<InvoiceRow> = self.session.select((TABLE, id)).await.map_err(map_err)?;
        row.map(Invoice::try_from).transpose()
    }

    async fn create(&self, data: NewInvoice, tenant: &Tenant) -> Result<Invoice, DomainError> {
        let order: Option<OrderSnapshotRow> = self
            .session
            .select((ORDER_TABLE, data.order_id.as_str()))
            .await
            .map_err(map_err)?;
        let order = order.ok_or_else(order_not_found_error)?;

        let order_status = order_status_from_db(order.status)?;
        if !can_invoice(order_status) {
            return Err(DomainError::Conflict(
                ConflictReason::OrderNotConfirmedForInvoice,
            ));
        }
        if self.invoice_exists_for_order(&data.order_id).await? {
            return Err(DomainError::Conflict(ConflictReason::OrderAlreadyInvoiced));
        }
        if let Some(requested) = &data.currency
            && requested != &order.currency
        {
            return Err(currency_mismatch_error(&order.currency));
        }

        // Display-only conversion snapshot (PLAN.md M4): taken at creation,
        // not at issue — the invoice's currency is fixed from here on, so
        // there is nothing left to snapshot later. `None` when the order was
        // already priced in the tenant's own default currency, or when no
        // rate was seeded for this pair (informational feature only).
        let exchange_rate = if order.currency == tenant.default_currency {
            None
        } else {
            lookup_rate(&self.session, &tenant.default_currency, &order.currency).await?
        };

        let net_total = domain::Money::from(order.total);
        let tax_total = compute_tax(&net_total, DEFAULT_TAX_RATE_BP);
        let gross_total = compute_gross(&net_total, &tax_total);
        let number = next_number(&self.session, "invoice", &tenant.invoice_prefix).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();

        let content = InvoiceContent {
            number,
            order_id: data.order_id,
            customer_id: order.customer_id,
            status: status_to_str(InvoiceStatus::Draft).to_string(),
            currency: order.currency,
            exchange_rate,
            line_items: order.line_items,
            net_total: net_total.into(),
            tax_rate_bp: DEFAULT_TAX_RATE_BP,
            tax_total: tax_total.into(),
            gross_total: gross_total.into(),
            issue_date: None,
            due_date: None,
            created_at: now.clone(),
            updated_at: now,
        };

        let row: Option<InvoiceRow> = self
            .session
            .create((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;

        row.map(Invoice::try_from)
            .transpose()?
            .ok_or_else(|| DomainError::Store("invoice create returned no row".to_string()))
    }

    async fn update(&self, id: &str, data: UpdateInvoice) -> Result<Invoice, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        if !can_edit(existing.status) {
            return Err(DomainError::Conflict(ConflictReason::InvoiceNotDraft));
        }
        validate_line_item_currencies(&data.line_items, &existing.currency)?;

        let net_total = line_items_total(&data.line_items, &existing.currency);
        let tax_total = compute_tax(&net_total, existing.tax_rate_bp);
        let gross_total = compute_gross(&net_total, &tax_total);
        let now = chrono::Utc::now().to_rfc3339();

        let content = InvoiceContent {
            number: existing.number,
            order_id: existing.order_id,
            customer_id: existing.customer_id,
            status: status_to_str(existing.status).to_string(),
            currency: existing.currency,
            exchange_rate: existing.exchange_rate,
            line_items: data.line_items.into_iter().map(LineItemRow::from).collect(),
            net_total: net_total.into(),
            tax_rate_bp: existing.tax_rate_bp,
            tax_total: tax_total.into(),
            gross_total: gross_total.into(),
            issue_date: existing.issue_date,
            due_date: existing.due_date,
            created_at: existing.created_at,
            updated_at: now,
        };

        let row: Option<InvoiceRow> = self
            .session
            .update((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;

        row.map(Invoice::try_from)
            .transpose()?
            .ok_or(DomainError::NotFound)
    }

    async fn delete(&self, _id: &str) -> Result<(), DomainError> {
        Err(DomainError::Conflict(
            ConflictReason::InvoiceCannotBeDeleted,
        ))
    }

    async fn set_status(&self, id: &str, status: InvoiceStatus) -> Result<Invoice, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        validate_transition(existing.status, status)?;

        let now = chrono::Utc::now().to_rfc3339();
        let row: Option<InvoiceRow> = if status == InvoiceStatus::Issued {
            let issue_date = chrono::Utc::now().date_naive();
            let due_date = due_date_from_issue(issue_date);
            let patch = IssuePatch {
                status: status_to_str(status).to_string(),
                issue_date: issue_date.to_string(),
                due_date: due_date.to_string(),
                updated_at: now,
            };
            self.session
                .update((TABLE, id))
                .merge(patch)
                .await
                .map_err(map_err)?
        } else {
            let patch = StatusPatch {
                status: status_to_str(status).to_string(),
                updated_at: now,
            };
            self.session
                .update((TABLE, id))
                .merge(patch)
                .await
                .map_err(map_err)?
        };

        row.map(Invoice::try_from)
            .transpose()?
            .ok_or(DomainError::NotFound)
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
