use async_trait::async_trait;
use domain::customer::{Address, Customer, CustomerRepo, ListQuery, NewCustomer, Paged};
use domain::error::DomainError;
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use ulid::Ulid;

const TABLE: &str = "customer";
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
struct CustomerRow {
    id: RecordId,
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
        details.insert("sort".to_string(), format!("unknown sort field: {field}"));
        return Err(DomainError::Validation(details));
    }
    Ok(format!("{field} {dir}"))
}

pub struct SurrealCustomerRepo {
    session: Surreal<Any>,
}

impl SurrealCustomerRepo {
    pub fn new(session: Surreal<Any>) -> Self {
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
        let order = sort_clause(&query.sort)?;
        let start = (query.page.saturating_sub(1) as i64) * query.limit as i64;

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

        Ok(Paged {
            items: rows.into_iter().map(Customer::from).collect(),
            total,
            page: query.page,
            limit: query.limit,
        })
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
            return Err(DomainError::Conflict(
                "customer has orders and cannot be deleted".to_string(),
            ));
        }
        let row: Option<CustomerRow> = self.session.delete((TABLE, id)).await.map_err(map_err)?;
        row.map(|_| ()).ok_or(DomainError::NotFound)
    }
}
