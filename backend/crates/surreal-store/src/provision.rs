use std::collections::HashMap;
use std::sync::Arc;

use domain::error::DomainError;
use domain::tenant::{NewTenant, Tenant, TenantRepo};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use crate::migrations::apply_migrations;
use crate::store::Store;
use crate::tenant_repo::SurrealTenantRepo;

fn tenant_db_name(org_id: &str) -> String {
    let digest = Sha256::digest(org_id.as_bytes());
    let hex = hex::encode(digest);
    format!("tenant_{}", &hex[..12])
}

/// Provisions a tenant database on first sight of an org id, guarded by a
/// per-org-id async mutex so concurrent first requests don't provision the
/// same tenant twice.
pub struct TenantProvisioner {
    store: Arc<Store>,
    locks: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl TenantProvisioner {
    pub fn new(store: Arc<Store>) -> Self {
        Self {
            store,
            locks: Mutex::new(HashMap::new()),
        }
    }

    async fn lock_for(&self, org_id: &str) -> Arc<Mutex<()>> {
        let mut locks = self.locks.lock().await;
        locks
            .entry(org_id.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    pub async fn ensure_tenant(
        &self,
        org_id: &str,
        display_name: &str,
    ) -> Result<Tenant, DomainError> {
        let registry = SurrealTenantRepo::new(self.store.system());

        if let Some(tenant) = registry.find_by_org_id(org_id).await? {
            return Ok(tenant);
        }

        let org_lock = self.lock_for(org_id).await;
        let _guard = org_lock.lock().await;

        // Re-check now that we hold the org-scoped lock: another request may
        // have provisioned the tenant while we were waiting.
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
