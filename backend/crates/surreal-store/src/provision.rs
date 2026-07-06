use std::sync::Arc;
use std::time::Duration;

use domain::error::DomainError;
use domain::tenant::{NewTenant, Tenant, TenantRepo};
use sha2::{Digest, Sha256};

use crate::migrations::apply_migrations;
use crate::store::Store;
use crate::tenant_repo::SurrealTenantRepo;

fn tenant_db_name(org_id: &str) -> String {
    let digest = Sha256::digest(org_id.as_bytes());
    let hex = hex::encode(digest);
    format!("tenant_{}", &hex[..12])
}

// Staleness window for tenant settings (name, default language/currency):
// an admin's change to a warm org id can take up to this long to show up.
// `invalidate` bypasses the wait for the write path itself once one exists;
// this is the backstop for every other reader.
const TENANT_CACHE_TTL: Duration = Duration::from_secs(60);

/// Caches `org_id -> Tenant` so authenticated requests don't hit the system
/// db on every call, and provisions a tenant database on first sight of an
/// org id. `try_get_with` coalesces concurrent lookups of the same key onto
/// a single in-flight load, which doubles as the provisioning guard that a
/// hand-rolled per-org-id lock used to provide.
pub struct TenantProvisioner {
    store: Arc<Store>,
    cache: moka::future::Cache<String, Tenant>,
}

impl TenantProvisioner {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            cache: moka::future::Cache::builder()
                .max_capacity(10_000)
                .time_to_live(TENANT_CACHE_TTL)
                .build(),
        }
    }

    pub async fn ensure_tenant(
        &self,
        org_id: &str,
        display_name: &str,
    ) -> Result<Tenant, DomainError> {
        self.cache
            .try_get_with(org_id.to_string(), self.find_or_provision(org_id, display_name))
            .await
            .map_err(|err| (*err).clone())
    }

    /// Drops a cached entry immediately, e.g. after a tenant settings write,
    /// instead of waiting out `TENANT_CACHE_TTL`.
    pub async fn invalidate(&self, org_id: &str) {
        self.cache.invalidate(org_id).await;
    }

    async fn find_or_provision(
        &self,
        org_id: &str,
        display_name: &str,
    ) -> Result<Tenant, DomainError> {
        let registry = SurrealTenantRepo::new(self.store.system());

        if let Some(tenant) = registry.find_by_org_id(org_id).await? {
            return Ok(tenant);
        }

        let db_name = tenant_db_name(org_id);
        let tenant_session = self
            .store
            .for_tenant(&db_name)
            .await
            .map_err(|e| DomainError::Store(e.to_string()))?;
        apply_migrations(&tenant_session)
            .await
            .map_err(|e| DomainError::Store(e.to_string()))?;

        registry
            .create(NewTenant {
                org_id: org_id.to_string(),
                db_name,
                name: display_name.to_string(),
            })
            .await
    }
}
