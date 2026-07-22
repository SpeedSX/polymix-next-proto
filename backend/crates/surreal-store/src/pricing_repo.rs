//! SurrealDB-backed pricing catalog (A2a-3).
//!
//! Catalog documents are stored as the `quote-engine` shapes verbatim — no
//! parallel row DTOs. `serde_json::Value` implements `SurrealValue`, so a
//! document round-trips through `.content(...)` / `SELECT *` with the engine's
//! own serde as the single source of truth for the wire shape. Record ids
//! (`format:a5`) come back as strings via SurrealDB's JSON projection, matching
//! the engine's opaque-string id contract (spec §1).
//!
//! Every mutation bumps `meta:pricing.version` inside the same transaction
//! (spec §2) — the invariant that makes quotes auditable.

use std::sync::Arc;

use async_trait::async_trait;
use domain::error::DomainError;
use domain::pricing::{PricingEntity, PricingRepo, validate};
use quote_engine::Dataset;
use serde_json::{Value, json};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use ulid::Ulid;

const VERSION_TABLE: &str = "meta";
const VERSION_ID: &str = "pricing";

/// Write a document and bump the version in one transaction. `$tb`/`$key`
/// target the record; `$content` is the document minus its `id` (the record
/// key is authoritative). Bind these and run against the tenant session.
const CREATE_TXN: &str = "BEGIN; \
    CREATE type::record($tb, $key) CONTENT $content; \
    UPSERT meta:pricing SET version = (version ?? 0) + 1; \
    COMMIT;";
const UPDATE_TXN: &str = "BEGIN; \
    UPDATE type::record($tb, $key) CONTENT $content; \
    UPSERT meta:pricing SET version = (version ?? 0) + 1; \
    COMMIT;";
const DELETE_TXN: &str = "BEGIN; \
    DELETE type::record($tb, $key); \
    UPSERT meta:pricing SET version = (version ?? 0) + 1; \
    COMMIT;";

fn map_err(err: surrealdb::Error) -> DomainError {
    DomainError::Store(err.to_string())
}

fn invalid_shape() -> DomainError {
    DomainError::Validation(std::collections::HashMap::from([(
        "_".to_string(),
        domain::error::FieldError::code("invalid_shape"),
    )]))
}

/// The bare record key for a SurrealDB target, tolerating either the full
/// record id (`format:a5`, as returned to clients) or a bare key (`a5`).
fn record_key(entity: PricingEntity, id: &str) -> String {
    let prefix = format!("{}:", entity.table());
    id.strip_prefix(&prefix).unwrap_or(id).to_string()
}

/// Inject the server-authoritative `id` into a create/update payload and split
/// off the content (document without `id`) to store under the record key.
fn prepare(
    entity: PricingEntity,
    mut doc: Value,
    key: &str,
) -> Result<(Value, Value), DomainError> {
    let full_id = format!("{}:{}", entity.table(), key);
    let obj = doc.as_object_mut().ok_or_else(invalid_shape)?;
    obj.insert("id".to_string(), Value::String(full_id));
    validate(entity, &doc)?;
    let mut content = doc.clone();
    content
        .as_object_mut()
        .expect("doc is an object")
        .remove("id");
    Ok((doc, content))
}

pub struct SurrealPricingRepo {
    session: Arc<Surreal<Any>>,
}

impl SurrealPricingRepo {
    pub fn new(session: Arc<Surreal<Any>>) -> Self {
        Self { session }
    }

    async fn exists(&self, entity: PricingEntity, key: &str) -> Result<bool, DomainError> {
        let row: Option<Value> = self
            .session
            .select((entity.table(), key))
            .await
            .map_err(map_err)?;
        Ok(row.is_some())
    }
}

#[async_trait]
impl PricingRepo for SurrealPricingRepo {
    async fn list(&self, entity: PricingEntity) -> Result<Vec<Value>, DomainError> {
        // `sort_field` is a closed-set `&'static str`, never user input — safe
        // to interpolate (SurrealQL can't bind an ORDER BY identifier).
        let query = format!(
            "SELECT * FROM type::table($tb) ORDER BY {} ASC",
            entity.sort_field()
        );
        let mut response = self
            .session
            .query(query)
            .bind(("tb", entity.table()))
            .await
            .map_err(map_err)?;
        response.take(0).map_err(map_err)
    }

    async fn get(&self, entity: PricingEntity, id: &str) -> Result<Option<Value>, DomainError> {
        let key = record_key(entity, id);
        self.session
            .select((entity.table(), key))
            .await
            .map_err(map_err)
    }

    async fn create(&self, entity: PricingEntity, doc: Value) -> Result<Value, DomainError> {
        let key = Ulid::new().to_string();
        let (stored, content) = prepare(entity, doc, &key)?;
        self.session
            .query(CREATE_TXN)
            .bind(("tb", entity.table()))
            .bind(("key", key))
            .bind(("content", content))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        Ok(stored)
    }

    async fn update(
        &self,
        entity: PricingEntity,
        id: &str,
        doc: Value,
    ) -> Result<Value, DomainError> {
        let key = record_key(entity, id);
        if !self.exists(entity, &key).await? {
            return Err(DomainError::NotFound);
        }
        let (stored, content) = prepare(entity, doc, &key)?;
        self.session
            .query(UPDATE_TXN)
            .bind(("tb", entity.table()))
            .bind(("key", key))
            .bind(("content", content))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        Ok(stored)
    }

    async fn delete(&self, entity: PricingEntity, id: &str) -> Result<(), DomainError> {
        let key = record_key(entity, id);
        // No delete guard in A2a: nothing in the five catalog tables references
        // another. The reference check (a catalog row used by a template
        // effect) lands with the template editor in A2b, per the pinned
        // decision in docs/pricing-admin-plan.md.
        if !self.exists(entity, &key).await? {
            return Err(DomainError::NotFound);
        }
        self.session
            .query(DELETE_TXN)
            .bind(("tb", entity.table()))
            .bind(("key", key))
            .await
            .map_err(map_err)?
            .check()
            .map_err(map_err)?;
        Ok(())
    }

    async fn get_version(&self) -> Result<i64, DomainError> {
        let row: Option<Value> = self
            .session
            .select((VERSION_TABLE, VERSION_ID))
            .await
            .map_err(map_err)?;
        // Absent only on a tenant that predates the 0012 migration between its
        // table-define and version-init statements — treat as the initial 1.
        Ok(row
            .as_ref()
            .and_then(|r| r.get("version"))
            .and_then(Value::as_i64)
            .unwrap_or(1))
    }

    async fn load_dataset(&self) -> Result<Dataset, DomainError> {
        // Read the version, then the tables. A concurrent mutation between the
        // two reads shows up as a version mismatch the snapshot cache checks
        // for (docs/pricing-admin-plan.md A2a-4); here we just tag the dataset
        // with the version we observed.
        let version = self.get_version().await?;
        let ds = json!({
            "pricelist_version": version,
            "formats": self.list(PricingEntity::Format).await?,
            "materials": self.list(PricingEntity::Material).await?,
            "machines": self.list(PricingEntity::Machine).await?,
            "operations": self.list(PricingEntity::Operation).await?,
            "pricing_policies": self.list(PricingEntity::Policy).await?,
        });
        serde_json::from_value(ds).map_err(|e| DomainError::Store(format!("dataset decode: {e}")))
    }
}
