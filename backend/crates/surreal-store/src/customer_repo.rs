use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use domain::customer::{Address, Customer, CustomerRepo, ListQuery, NewCustomer, Paged};
use domain::error::{ConflictReason, DomainError, FieldError};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use ulid::Ulid;

pub(crate) const TABLE: &str = "customer";
const ORDER_TABLE: &str = "order";

// Whitelisted, not bound as a query parameter: SurrealQL identifiers (unlike
// values) can't be passed as bind parameters, so the sort field is validated
// against this list before being interpolated into the ORDER BY clause.
const ALLOWED_SORT_FIELDS: &[&str] = &[
    "name",
    "contact_name",
    "email",
    "phone",
    "created_at",
    "updated_at",
];

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

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
pub(crate) struct CustomerRow {
    id: RecordId,
    name: String,
    contact_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    address: Option<AddressRow>,
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
    name: String,
    contact_name: Option<String>,
    email: Option<String>,
    phone: Option<String>,
    address: Option<AddressRow>,
    notes: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct CountRow {
    count: i64,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct IdOnly {
    #[allow(dead_code)]
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

impl From<CustomerRow> for Customer {
    fn from(row: CustomerRow) -> Self {
        Customer {
            id: record_key(&row.id),
            name: row.name,
            contact_name: row.contact_name,
            email: row.email,
            phone: row.phone,
            address: row.address.map(Address::from),
            notes: row.notes,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

fn content_from(data: NewCustomer, created_at: String, updated_at: String) -> CustomerContent {
    CustomerContent {
        name: data.name,
        contact_name: data.contact_name,
        email: data.email,
        phone: data.phone,
        address: data.address.map(AddressRow::from),
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

// The searchable fields, matching migrations/0004_search.surql's per-field
// FULLTEXT indexes. Each field is queried as its OWN statement (all sent in
// one request) and the results merged in Rust, instead of one
// `field1 @0@ $q OR field2 @1@ $q ...` query: the OR form costs ~105ms
// server-side for a common prefix on the seeded 50k-customer tenant, while
// the same predicates as separate statements cost ~10-20ms each —
// SurrealDB 3.2 can push the LIMIT into a single-index FullTextScan but not
// into a multi-index OR. Measured in examples/perf_probe.rs; see
// docs/adr/0006-tenant-session-cache-and-search-split.md.
const SEARCH_FIELDS: &[&str] = &["name", "contact_name", "email"];

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
    async fn list(&self, query: ListQuery) -> Result<Paged<Customer>, DomainError> {
        let start = (query.page.saturating_sub(1) as i64) * query.limit as i64;

        let (rows, total) = if let Some(q) = non_empty_q(&query.q) {
            // One statement per searchable field plus one count per field,
            // all in a single round-trip; see SEARCH_FIELDS for why this
            // beats a multi-index OR. The count subquery wrap works around
            // the zero-count planner bug (docs/adr/0001).
            let window = (start + query.limit as i64).min(MAX_SEARCH_WINDOW);
            let mut statements = String::new();
            for field in SEARCH_FIELDS {
                statements.push_str(&format!(
                    "SELECT *, search::score(0) AS score FROM type::table($table) \
                     WHERE {field} @0@ $q ORDER BY score DESC LIMIT $window;"
                ));
            }
            // Id-only projections for the exact total: each is a fast
            // single-index scan; the union is deduped in Rust because
            // per-field counts would double-count rows matching several
            // fields, and pushing the union into the query loses the fast
            // path (measured ~62ms vs ~24ms for these statements).
            for field in SEARCH_FIELDS {
                statements.push_str(&format!(
                    "SELECT VALUE id FROM type::table($table) WHERE {field} @0@ $q;"
                ));
            }
            let mut response = self
                .session
                .query(statements)
                .bind(("table", TABLE))
                .bind(("q", q.to_string()))
                .bind(("window", window))
                .await
                .map_err(map_err)?;

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
            let mut response = self
                .session
                .query(format!(
                    "SELECT * FROM type::table($table) ORDER BY {order} LIMIT $limit START $start"
                ))
                .bind(("table", TABLE))
                .bind(("limit", query.limit as i64))
                .bind(("start", start))
                .await
                .map_err(map_err)?;
            let rows: Vec<CustomerRow> = response.take(0).map_err(map_err)?;

            let mut count_response = self
                .session
                .query("SELECT count() FROM type::table($table) GROUP ALL")
                .bind(("table", TABLE))
                .await
                .map_err(map_err)?;
            let count_rows: Vec<CountRow> = count_response.take(0).map_err(map_err)?;
            let total = count_rows.first().map(|r| r.count as u64).unwrap_or(0);
            (rows, total)
        };

        Ok(Paged {
            items: rows.into_iter().map(Customer::from).collect(),
            total,
            page: query.page,
            limit: query.limit,
        })
    }

    async fn search(&self, q: &str, limit: u32) -> Result<Vec<domain::SearchHit>, DomainError> {
        // One statement per field, merged in Rust — see SEARCH_FIELDS.
        // `search::highlight(..., 0)` in each statement highlights that
        // statement's own field, so a contact_name/email match shows the
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

    async fn get(&self, id: &str) -> Result<Option<Customer>, DomainError> {
        let row: Option<CustomerRow> = self.session.select((TABLE, id)).await.map_err(map_err)?;
        Ok(row.map(Customer::from))
    }

    async fn create(&self, data: NewCustomer) -> Result<Customer, DomainError> {
        let now = chrono::Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        let content = content_from(data, now.clone(), now);

        let row: Option<CustomerRow> = self
            .session
            .create((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;

        row.map(Customer::from)
            .ok_or_else(|| DomainError::Store("customer create returned no row".to_string()))
    }

    async fn update(&self, id: &str, data: NewCustomer) -> Result<Customer, DomainError> {
        let existing = self.get(id).await?.ok_or(DomainError::NotFound)?;
        let now = chrono::Utc::now().to_rfc3339();
        let content = content_from(data, existing.created_at, now);

        let row: Option<CustomerRow> = self
            .session
            .update((TABLE, id))
            .content(content)
            .await
            .map_err(map_err)?;

        row.map(Customer::from).ok_or(DomainError::NotFound)
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        if self.has_orders(id).await? {
            return Err(DomainError::Conflict(ConflictReason::CustomerHasOrders));
        }
        let row: Option<CustomerRow> = self.session.delete((TABLE, id)).await.map_err(map_err)?;
        row.map(|_| ()).ok_or(DomainError::NotFound)
    }
}
