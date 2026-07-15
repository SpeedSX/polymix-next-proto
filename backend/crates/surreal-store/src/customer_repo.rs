use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use domain::customer::{
    Address, Contact, Customer, CustomerRepo, CustomerStatus, ListQuery, NewCustomer, Paged,
    validate_transition,
};
use domain::error::{ConflictReason, DomainError, FieldError};
use domain::tenant::Tenant;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use ulid::Ulid;

use crate::counter::next_number;
use crate::order_repo::MoneyRow;
use crate::status::{customer_kind_from_db, customer_status_from_db};

pub(crate) const TABLE: &str = "customer";
const ORDER_TABLE: &str = "order";

// Whitelisted, not bound as a query parameter: SurrealQL identifiers (unlike
// values) can't be passed as bind parameters, so the sort field is validated
// against this list before being interpolated into the ORDER BY clause.
const ALLOWED_SORT_FIELDS: &[&str] = &["name", "number", "status", "created_at", "updated_at"];

#[derive(Debug, Clone, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct AddressRow {
    street: Option<String>,
    zip: Option<String>,
    city: Option<String>,
    country: Option<String>,
}

impl From<Address> for AddressRow {
    fn from(a: Address) -> Self {
        AddressRow {
            street: a.street,
            zip: a.zip,
            city: a.city,
            country: a.country,
        }
    }
}

impl From<AddressRow> for Address {
    fn from(a: AddressRow) -> Self {
        Address {
            street: a.street,
            zip: a.zip,
            city: a.city,
            country: a.country,
        }
    }
}

#[derive(Debug, Clone, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct ContactRow {
    name: String,
    role: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    is_primary: bool,
}

impl From<Contact> for ContactRow {
    fn from(c: Contact) -> Self {
        ContactRow {
            name: c.name,
            role: c.role,
            email: c.email,
            phone: c.phone,
            is_primary: c.is_primary,
        }
    }
}

impl From<ContactRow> for Contact {
    fn from(c: ContactRow) -> Self {
        Contact {
            name: c.name,
            role: c.role,
            email: c.email,
            phone: c.phone,
            is_primary: c.is_primary,
        }
    }
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct CustomerRow {
    id: RecordId,
    number: String,
    kind: i64,
    name: String,
    legal_name: Option<String>,
    edrpou: Option<String>,
    tax_id: Option<String>,
    vat_ipn: Option<String>,
    status: i64,
    tags: Vec<String>,
    industry: Option<String>,
    source: Option<String>,
    website: Option<String>,
    contacts: Vec<ContactRow>,
    legal_address: Option<AddressRow>,
    delivery_address: Option<AddressRow>,
    payment_terms_days: u16,
    credit_limit: Option<MoneyRow>,
    /// `None` for rows migrated before M5.1 that were never rewritten since
    /// (the migration deliberately doesn't backfill this — see
    /// `docs/customers-crm.md`) — repaired to the tenant default at read
    /// time by `customer_from_row`, not stored back.
    default_currency: Option<String>,
    default_discount_bp: u16,
    iban: Option<String>,
    bank_name: Option<String>,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
    /// BM25 score projected by the per-field search statements; absent on
    /// every non-search read.
    score: Option<f64>,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CustomerContent {
    number: String,
    kind: i64,
    name: String,
    legal_name: Option<String>,
    edrpou: Option<String>,
    tax_id: Option<String>,
    vat_ipn: Option<String>,
    status: i64,
    tags: Vec<String>,
    industry: Option<String>,
    source: Option<String>,
    website: Option<String>,
    contacts: Vec<ContactRow>,
    legal_address: Option<AddressRow>,
    delivery_address: Option<AddressRow>,
    payment_terms_days: u16,
    credit_limit: Option<MoneyRow>,
    default_currency: Option<String>,
    default_discount_bp: u16,
    iban: Option<String>,
    bank_name: Option<String>,
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
struct NumberPatch {
    number: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CountRow {
    count: i64,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct IdOnly {
    id: RecordId,
}

fn record_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(key) => key.clone(),
        other => format!("{other:?}"),
    }
}

impl CustomerRow {
    pub(crate) fn key(&self) -> String {
        record_key(&self.id)
    }
}

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

fn customer_from_row_with_currency(
    row: CustomerRow,
    fallback_currency: &str,
) -> Result<Customer, DomainError> {
    Ok(Customer {
        id: record_key(&row.id),
        number: row.number,
        kind: customer_kind_from_db(row.kind)?,
        name: row.name,
        legal_name: row.legal_name,
        edrpou: row.edrpou,
        tax_id: row.tax_id,
        vat_ipn: row.vat_ipn,
        status: customer_status_from_db(row.status)?,
        tags: row.tags,
        industry: row.industry,
        source: row.source,
        website: row.website,
        contacts: row.contacts.into_iter().map(Contact::from).collect(),
        legal_address: row.legal_address.map(Address::from),
        delivery_address: row.delivery_address.map(Address::from),
        payment_terms_days: row.payment_terms_days,
        credit_limit: row.credit_limit.map(domain::Money::from),
        default_currency: row
            .default_currency
            .unwrap_or_else(|| fallback_currency.to_string()),
        default_discount_bp: row.default_discount_bp,
        iban: row.iban,
        bank_name: row.bank_name,
        notes: row.notes,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

fn customer_from_row(row: CustomerRow, tenant: &Tenant) -> Result<Customer, DomainError> {
    customer_from_row_with_currency(row, &tenant.default_currency)
}

/// Used by the live-change stream (`live.rs`), which has no `Tenant` in
/// scope (it's a per-tenant-db session with no cheap path back to the
/// tenant registry). Falls back to an empty string for the rare legacy row
/// whose `default_currency` was never backfilled and hasn't been rewritten
/// since the M5.1 migration — the request path (`customer_from_row`)
/// repairs it properly on the very next read of that row.
pub(crate) fn customer_from_row_untenanted(row: CustomerRow) -> Result<Customer, DomainError> {
    customer_from_row_with_currency(row, "")
}

fn content_from(
    data: NewCustomer,
    number: String,
    status: CustomerStatus,
    created_at: String,
    updated_at: String,
) -> CustomerContent {
    CustomerContent {
        number,
        kind: data.kind.code() as i64,
        name: data.name,
        legal_name: data.legal_name,
        edrpou: data.edrpou,
        tax_id: data.tax_id,
        vat_ipn: data.vat_ipn,
        status: status.code() as i64,
        tags: data.tags,
        industry: data.industry,
        source: data.source,
        website: data.website,
        contacts: data.contacts.into_iter().map(ContactRow::from).collect(),
        legal_address: data.legal_address.map(AddressRow::from),
        delivery_address: data.delivery_address.map(AddressRow::from),
        payment_terms_days: data.payment_terms_days,
        credit_limit: data.credit_limit.map(MoneyRow::from),
        default_currency: data.default_currency,
        default_discount_bp: data.default_discount_bp,
        iban: data.iban,
        bank_name: data.bank_name,
        notes: data.notes,
        created_at,
        updated_at,
    }
}

fn sort_clause(sort: &str) -> Result<String, DomainError> {
    let (field, dir) = match sort.strip_prefix('-') {
        Some(field) => (field, "DESC"),
        None => (sort, "ASC"),
    };
    if !ALLOWED_SORT_FIELDS.contains(&field) {
        let mut details = std::collections::HashMap::new();
        details.insert(
            "sort".to_string(),
            FieldError::with_params(
                "unknown_sort_field",
                std::collections::HashMap::from([("field".to_string(), field.to_string())]),
            ),
        );
        return Err(DomainError::Validation(details));
    }
    Ok(format!("{field} {dir}"))
}

fn non_empty_q(q: &Option<String>) -> Option<&str> {
    q.as_deref().filter(|s| !s.is_empty())
}

/// Non-full-text filters, shared by the plain-list and the per-field search
/// paths — see `where_clause` (plain path, `WHERE ... AND ...`) and
/// `extra_and` (search path, appended to each per-field predicate).
fn status_tag_conditions(query: &ListQuery) -> Vec<&'static str> {
    let mut conditions = Vec::new();
    if query.status.is_some() {
        conditions.push("status = $status");
    }
    if query.tag.is_some() {
        conditions.push("tags CONTAINS $tag");
    }
    conditions
}

fn where_clause(query: &ListQuery) -> String {
    let conditions = status_tag_conditions(query);
    if conditions.is_empty() {
        String::new()
    } else {
        format!("WHERE {}", conditions.join(" AND "))
    }
}

fn extra_and(query: &ListQuery) -> String {
    let conditions = status_tag_conditions(query);
    if conditions.is_empty() {
        String::new()
    } else {
        format!(" AND {}", conditions.join(" AND "))
    }
}

// The searchable fields, matching migrations/0009_customers_crm.surql's
// per-field FULLTEXT indexes. Each field is queried as its OWN statement (all
// sent in one request) and the results merged in Rust, instead of one
// `field1 @0@ $q OR field2 @1@ $q ...` query: the OR form costs ~105ms
// server-side for a common prefix on the seeded 50k-customer tenant, while
// the same predicates as separate statements cost ~10-20ms each —
// SurrealDB 3.2 can push the LIMIT into a single-index FullTextScan but not
// into a multi-index OR. Measured in examples/perf_probe.rs; see
// docs/adr/0006-tenant-session-cache-and-search-split.md.
const SEARCH_FIELDS: &[&str] = &[
    "name",
    "legal_name",
    "number",
    "edrpou",
    "contacts[*].name",
    "contacts[*].email",
];

/// Caps how many rows each per-field search statement returns for the
/// paged list: deep pagination over merged rankings would otherwise force
/// every field to materialize `page × limit` rows.
const MAX_SEARCH_WINDOW: i64 = 1_000;

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct SearchHitRow {
    id: RecordId,
    label: String,
    highlight: Option<String>,
    score: Option<f64>,
}

fn to_hit(row: SearchHitRow) -> domain::SearchHit {
    domain::SearchHit {
        id: record_key(&row.id),
        highlight: row.highlight.unwrap_or_else(|| row.label.clone()),
        label: row.label,
    }
}

/// Merges per-field ranked results: dedupes by record id, SUMMING the
/// per-field scores — the same combined-relevance semantics as the previous
/// single-query `(search::score(0) + … + search::score(3))` form, which the
/// multi-field-outranks-single-field integration test pins. A component
/// score is lost when a row places outside that field's LIMIT window, so
/// ranking near the window edge is approximate. Ties break on id so
/// pagination stays deterministic.
fn merge_ranked<T>(
    per_field: Vec<Vec<T>>,
    key: impl Fn(&T) -> String,
    score: impl Fn(&T) -> f64,
) -> Vec<T> {
    let mut best: HashMap<String, (T, f64)> = HashMap::new();
    for row in per_field.into_iter().flatten() {
        let k = key(&row);
        let s = score(&row);
        match best.get_mut(&k) {
            Some((_, sum)) => *sum += s,
            None => {
                best.insert(k, (row, s));
            }
        }
    }
    let mut merged: Vec<(T, f64)> = best.into_values().collect();
    merged.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| key(&a.0).cmp(&key(&b.0))));
    merged.into_iter().map(|(row, _)| row).collect()
}

pub struct SurrealCustomerRepo {
    session: Arc<Surreal<Any>>,
}

impl SurrealCustomerRepo {
    pub fn new(session: Arc<Surreal<Any>>) -> Self {
        Self { session }
    }

    async fn get_row(&self, id: &str) -> Result<Option<CustomerRow>, DomainError> {
        self.session.select((TABLE, id)).await.map_err(map_err)
    }

    async fn has_orders(&self, customer_id: &str) -> Result<bool, DomainError> {
        let mut response = self
            .session
            .query("SELECT id FROM type::table($table) WHERE customer_id = $customer_id LIMIT 1")
            .bind(("table", ORDER_TABLE))
            .bind(("customer_id", customer_id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<IdOnly> = response.take(0).map_err(map_err)?;
        Ok(!rows.is_empty())
    }
}

#[async_trait]
impl CustomerRepo for SurrealCustomerRepo {
    async fn list(
        &self,
        query: ListQuery,
        tenant: &Tenant,
    ) -> Result<Paged<Customer>, DomainError> {
        let start = (query.page.saturating_sub(1) as i64) * query.limit as i64;

        let (rows, total) = if let Some(q) = non_empty_q(&query.q) {
            // One statement per searchable field plus one count per field,
            // all in a single round-trip; see SEARCH_FIELDS for why this
            // beats a multi-index OR. The count subquery wrap works around
            // the zero-count planner bug (docs/adr/0001).
            let extra = extra_and(&query);
            let window = (start + query.limit as i64).min(MAX_SEARCH_WINDOW);
            let mut statements = String::new();
            for field in SEARCH_FIELDS {
                statements.push_str(&format!(
                    "SELECT *, search::score(0) AS score FROM type::table($table) \
                     WHERE {field} @0@ $q{extra} ORDER BY score DESC LIMIT $window;"
                ));
            }
            // Id-only projections for the exact total: each is a fast
            // single-index scan; the union is deduped in Rust because
            // per-field counts would double-count rows matching several
            // fields, and pushing the union into the query loses the fast
            // path (measured ~62ms vs ~24ms for these statements).
            for field in SEARCH_FIELDS {
                statements.push_str(&format!(
                    "SELECT VALUE id FROM type::table($table) WHERE {field} @0@ $q{extra};"
                ));
            }
            let mut list_query = self
                .session
                .query(statements)
                .bind(("table", TABLE))
                .bind(("q", q.to_string()))
                .bind(("window", window));
            if let Some(status) = query.status {
                list_query = list_query.bind(("status", status.code() as i64));
            }
            if let Some(tag) = &query.tag {
                list_query = list_query.bind(("tag", tag.clone()));
            }
            let mut response = list_query.await.map_err(map_err)?;

            let mut per_field = Vec::with_capacity(SEARCH_FIELDS.len());
            for i in 0..SEARCH_FIELDS.len() {
                let rows: Vec<CustomerRow> = response.take(i).map_err(map_err)?;
                per_field.push(rows);
            }
            let mut matched_ids: std::collections::HashSet<String> =
                std::collections::HashSet::new();
            for i in 0..SEARCH_FIELDS.len() {
                let ids: Vec<RecordId> = response.take(SEARCH_FIELDS.len() + i).map_err(map_err)?;
                matched_ids.extend(ids.iter().map(record_key));
            }
            let total = matched_ids.len() as u64;

            let rows = merge_ranked(
                per_field,
                |row: &CustomerRow| record_key(&row.id),
                |row| row.score.unwrap_or(0.0),
            )
            .into_iter()
            .skip(start as usize)
            .take(query.limit as usize)
            .collect();
            (rows, total)
        } else {
            let order = sort_clause(&query.sort)?;
            let filters = where_clause(&query);
            let mut list_query = self
                .session
                .query(format!(
                    "SELECT * FROM type::table($table) {filters} ORDER BY {order} LIMIT $limit START $start"
                ))
                .bind(("table", TABLE))
                .bind(("limit", query.limit as i64))
                .bind(("start", start));
            if let Some(status) = query.status {
                list_query = list_query.bind(("status", status.code() as i64));
            }
            if let Some(tag) = &query.tag {
                list_query = list_query.bind(("tag", tag.clone()));
            }
            let mut response = list_query.await.map_err(map_err)?;
            let rows: Vec<CustomerRow> = response.take(0).map_err(map_err)?;

            let mut count_query = self
                .session
                .query(format!(
                    "SELECT count() FROM type::table($table) {filters} GROUP ALL"
                ))
                .bind(("table", TABLE));
            if let Some(status) = query.status {
                count_query = count_query.bind(("status", status.code() as i64));
            }
            if let Some(tag) = &query.tag {
                count_query = count_query.bind(("tag", tag.clone()));
            }
            let mut count_response = count_query.await.map_err(map_err)?;
            let count_rows: Vec<CountRow> = count_response.take(0).map_err(map_err)?;
            let total = count_rows.first().map(|r| r.count as u64).unwrap_or(0);
            (rows, total)
        };

        let items = rows
            .into_iter()
            .map(|row| customer_from_row(row, tenant))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(Paged {
            items,
            total,
            page: query.page,
            limit: query.limit,
        })
    }

    async fn search(&self, q: &str, limit: u32) -> Result<Vec<domain::SearchHit>, DomainError> {
        // One statement per field, merged in Rust — see SEARCH_FIELDS.
        // `search::highlight(..., 0)` in each statement highlights that
        // statement's own field, so a contact/edrpou match shows the
        // matched fragment instead of falling back to the plain label.
        let mut statements = String::new();
        for field in SEARCH_FIELDS {
            statements.push_str(&format!(
                "SELECT id, name AS label, search::highlight('<b>', '</b>', 0) AS highlight, \
                 search::score(0) AS score FROM type::table($table) \
                 WHERE {field} @0@ $q ORDER BY score DESC LIMIT $limit;"
            ));
        }
        let mut response = self
            .session
            .query(statements)
            .bind(("table", TABLE))
            .bind(("q", q.to_string()))
            .bind(("limit", limit as i64))
            .await
            .map_err(map_err)?;
        let mut per_field = Vec::with_capacity(SEARCH_FIELDS.len());
        for i in 0..SEARCH_FIELDS.len() {
            let rows: Vec<SearchHitRow> = response.take(i).map_err(map_err)?;
            per_field.push(rows);
        }
        Ok(merge_ranked(
            per_field,
            |row: &SearchHitRow| record_key(&row.id),
            |row| row.score.unwrap_or(0.0),
        )
        .into_iter()
        .take(limit as usize)
        .map(to_hit)
        .collect())
    }

    async fn get(&self, id: &str, tenant: &Tenant) -> Result<Option<Customer>, DomainError> {
        self.get_row(id)
            .await?
            .map(|row| customer_from_row(row, tenant))
            .transpose()
    }

    async fn create(
        &self,
        mut data: NewCustomer,
        tenant: &Tenant,
    ) -> Result<Customer, DomainError> {
        let status = match data.status.take() {
            Some(0) => CustomerStatus::Lead,
            _ => CustomerStatus::Active,
        };
        let number = next_number(&self.session, "customer", &tenant.customer_prefix).await?;
        let now = chrono::Utc::now().to_rfc3339();
        let content = content_from(data, number, status, now.clone(), now);
        let id = Ulid::new().to_string();

        let row: Option<CustomerRow> = self
            .session
            .create((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;

        let row =
            row.ok_or_else(|| DomainError::Store("customer create returned no row".to_string()))?;
        customer_from_row(row, tenant)
    }

    async fn update(
        &self,
        id: &str,
        mut data: NewCustomer,
        tenant: &Tenant,
    ) -> Result<Customer, DomainError> {
        let existing = self.get_row(id).await?.ok_or(DomainError::NotFound)?;
        // Status changes only through `set_status` — a PUT body's `status`
        // (if any) is ignored entirely, matching how `number` is preserved.
        data.status = None;
        let now = chrono::Utc::now().to_rfc3339();
        let content = content_from(
            data,
            existing.number.clone(),
            customer_status_from_db(existing.status)?,
            existing.created_at.clone(),
            now,
        );

        let row: Option<CustomerRow> = self
            .session
            .update((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;

        let row = row.ok_or(DomainError::NotFound)?;
        customer_from_row(row, tenant)
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        if self.has_orders(id).await? {
            return Err(DomainError::Conflict(ConflictReason::CustomerHasOrders));
        }
        let row: Option<CustomerRow> = self.session.delete((TABLE, id)).await.map_err(map_err)?;
        row.map(|_| ()).ok_or(DomainError::NotFound)
    }

    async fn set_status(
        &self,
        id: &str,
        status: CustomerStatus,
        tenant: &Tenant,
    ) -> Result<Customer, DomainError> {
        let existing = self.get_row(id).await?.ok_or(DomainError::NotFound)?;
        validate_transition(customer_status_from_db(existing.status)?, status)?;

        let patch = StatusPatch {
            status: status.code() as i64,
            updated_at: chrono::Utc::now().to_rfc3339(),
        };
        let row: Option<CustomerRow> = self
            .session
            .update((TABLE, id))
            .merge(patch)
            .await
            .map_err(map_err)?;
        let row = row.ok_or(DomainError::NotFound)?;
        customer_from_row(row, tenant)
    }
}

/// One-time backfill for tenant databases migrated from before per-customer
/// numbering existed (`docs/customers-crm.md` migration step 4). Runs as
/// Rust rather than inline SurrealQL because there's no `string::format`-
/// style zero-pad builtin to lean on — this reuses `next_number`'s Rust
/// formatting instead of duplicating it in SQL. Idempotent: a no-op once
/// every row has a `number`. Called once per tenant at API startup,
/// alongside `migrations::apply_migrations` (see `api::build_state`).
pub async fn backfill_numbers(session: &Surreal<Any>) -> Result<(), DomainError> {
    let mut response = session
        // `created_at` must be in the projection: SurrealDB 3.2 rejects an
        // ORDER BY idiom that isn't also selected.
        .query(
            "SELECT id, created_at FROM type::table($table) WHERE number IS NONE ORDER BY created_at ASC",
        )
        .bind(("table", TABLE))
        .await
        .map_err(map_err)?;
    let rows: Vec<IdOnly> = response.take(0).map_err(map_err)?;
    for row in rows {
        let number = next_number(session, "customer", "").await?;
        let _: Option<IdOnly> = session
            .update((TABLE, record_key(&row.id)))
            .merge(NumberPatch { number })
            .await
            .map_err(map_err)?;
    }
    Ok(())
}
