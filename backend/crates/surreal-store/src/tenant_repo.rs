use async_trait::async_trait;
use domain::error::DomainError;
use domain::tenant::{NewTenant, Tenant, TenantRepo};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::types::{RecordId, RecordIdKey, SurrealValue};
use ulid::Ulid;

use crate::TENANT_ORG_ID_INDEX;

const TABLE: &str = "tenant";

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct TenantRow {
    id: RecordId,
    org_id: String,
    db_name: String,
    name: String,
    default_language: String,
    default_currency: String,
    // `Option` rather than `String`: tenants provisioned before M4 have no
    // value stored for these fields at all (SCHEMALESS table, no backfill
    // migration) — missing, not empty-string.
    order_prefix: Option<String>,
    invoice_prefix: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, SurrealValue)]
#[surreal(crate = "surrealdb::types")]
struct TenantContent {
    org_id: String,
    db_name: String,
    name: String,
    default_language: String,
    default_currency: String,
    order_prefix: String,
    invoice_prefix: String,
    created_at: String,
    updated_at: String,
}

fn record_key(id: &RecordId) -> String {
    match &id.key {
        RecordIdKey::String(key) => key.clone(),
        other => format!("{other:?}"),
    }
}

impl From<TenantRow> for Tenant {
    fn from(row: TenantRow) -> Self {
        Tenant {
            id: record_key(&row.id),
            org_id: row.org_id,
            db_name: row.db_name,
            name: row.name,
            default_language: row.default_language,
            default_currency: row.default_currency,
            order_prefix: row.order_prefix.unwrap_or_default(),
            invoice_prefix: row.invoice_prefix.unwrap_or_default(),
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

/// SurrealDB 3.2 doesn't map a unique-index violation to a structured
/// `AlreadyExists` error kind over the wire (confirmed empirically: it comes
/// back as a generic `Internal`-kind error) — the index name in the message
/// text is the only signal available, so match on that.
fn is_org_id_conflict(err: &surrealdb::Error) -> bool {
    let message = err.to_string();
    message.contains(TENANT_ORG_ID_INDEX) && message.contains("already contains")
}

pub struct SurrealTenantRepo {
    session: Surreal<Any>,
}

impl SurrealTenantRepo {
    pub fn new(session: Surreal<Any>) -> Self {
        Self { session }
    }
}

#[async_trait]
impl TenantRepo for SurrealTenantRepo {
    async fn find_by_org_id(&self, org_id: &str) -> Result<Option<Tenant>, DomainError> {
        let mut response = self
            .session
            .query("SELECT * FROM type::table($table) WHERE org_id = $org_id LIMIT 1")
            .bind(("table", TABLE))
            .bind(("org_id", org_id.to_string()))
            .await
            .map_err(map_err)?;
        let rows: Vec<TenantRow> = response.take(0).map_err(map_err)?;
        Ok(rows.into_iter().next().map(Tenant::from))
    }

    async fn list_all(&self) -> Result<Vec<Tenant>, DomainError> {
        let mut response = self
            .session
            .query("SELECT * FROM type::table($table)")
            .bind(("table", TABLE))
            .await
            .map_err(map_err)?;
        let rows: Vec<TenantRow> = response.take(0).map_err(map_err)?;
        Ok(rows.into_iter().map(Tenant::from).collect())
    }

    async fn create(&self, data: NewTenant) -> Result<Tenant, DomainError> {
        let now = chrono::Utc::now().to_rfc3339();
        let id = Ulid::new().to_string();
        let org_id = data.org_id.clone();
        let content = TenantContent {
            org_id: data.org_id,
            db_name: data.db_name,
            name: data.name,
            default_language: data.default_language,
            default_currency: data.default_currency,
            order_prefix: String::new(),
            invoice_prefix: String::new(),
            created_at: now.clone(),
            updated_at: now,
        };
        let result: Result<Option<TenantRow>, surrealdb::Error> =
            self.session.create((TABLE, id)).content(content).await;

        let row = match result {
            Ok(row) => row,
            // Lost a race with another process (or another instance) also
            // provisioning this org id — the unique index is what actually
            // caught it, since TenantProvisioner's cache only coalesces
            // lookups within this one process. Whoever won is the source of
            // truth; fetch it.
            Err(err) if is_org_id_conflict(&err) => {
                return self
                    .find_by_org_id(&org_id)
                    .await?
                    .ok_or_else(|| DomainError::Store(err.to_string()));
            }
            Err(err) => return Err(map_err(err)),
        };

        row.map(Tenant::from)
            .ok_or_else(|| DomainError::Store("tenant create returned no row".to_string()))
    }
}
