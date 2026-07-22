use std::sync::Arc;

use async_trait::async_trait;
use domain::{CustomerRepo, DomainError, InvoiceRepo, OrderRepo, Tenant, TenantRepo};
use surreal_store::{
    Store, SurrealCustomerRepo, SurrealInvoiceRepo, SurrealOrderRepo, SurrealTenantRepo,
    TenantProvisioner, migrations,
};

#[async_trait]
pub trait Backend: Send + Sync {
    async fn customer_repo(&self, tenant_db: &str) -> Result<Arc<dyn CustomerRepo>, DomainError>;
    async fn order_repo(&self, tenant_db: &str) -> Result<Arc<dyn OrderRepo>, DomainError>;
    async fn invoice_repo(&self, tenant_db: &str) -> Result<Arc<dyn InvoiceRepo>, DomainError>;
    fn tenant_repo(&self) -> Arc<dyn TenantRepo>;
    async fn provision_tenant(&self, org_id: &str, org_name: &str) -> Result<Tenant, DomainError>;
    async fn ping(&self) -> Result<(), DomainError>;
}

/// The only API-layer adapter that knows SurrealDB's concrete repository
/// types. Routes and authentication depend only on [`Backend`].
pub struct SurrealBackend {
    store: Arc<Store>,
    provisioner: TenantProvisioner,
}

impl SurrealBackend {
    pub async fn new(store: Arc<Store>) -> Result<Self, DomainError> {
        let tenant_repo = SurrealTenantRepo::new(store.system());
        for tenant in tenant_repo.list_all().await? {
            let session = store
                .for_tenant(&tenant.db_name)
                .await
                .map_err(|error| DomainError::Store(error.to_string()))?;
            migrations::apply_migrations(&session, &tenant.db_name)
                .await
                .map_err(|error| DomainError::Store(error.to_string()))?;
        }

        Ok(Self {
            provisioner: TenantProvisioner::new(store.clone()),
            store,
        })
    }
}

#[async_trait]
impl Backend for SurrealBackend {
    async fn customer_repo(&self, tenant_db: &str) -> Result<Arc<dyn CustomerRepo>, DomainError> {
        let session = self
            .store
            .for_tenant(tenant_db)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?;
        Ok(Arc::new(SurrealCustomerRepo::new(session)))
    }

    async fn order_repo(&self, tenant_db: &str) -> Result<Arc<dyn OrderRepo>, DomainError> {
        let session = self
            .store
            .for_tenant(tenant_db)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?;
        Ok(Arc::new(SurrealOrderRepo::new(session)))
    }

    async fn invoice_repo(&self, tenant_db: &str) -> Result<Arc<dyn InvoiceRepo>, DomainError> {
        let session = self
            .store
            .for_tenant(tenant_db)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?;
        Ok(Arc::new(SurrealInvoiceRepo::new(session)))
    }

    fn tenant_repo(&self) -> Arc<dyn TenantRepo> {
        Arc::new(SurrealTenantRepo::new(self.store.system()))
    }

    async fn provision_tenant(&self, org_id: &str, org_name: &str) -> Result<Tenant, DomainError> {
        self.provisioner.ensure_tenant(org_id, org_name).await
    }

    async fn ping(&self) -> Result<(), DomainError> {
        self.store
            .system()
            .query("RETURN true")
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?
            .check()
            .map_err(|error| DomainError::Store(error.to_string()))?;
        Ok(())
    }
}
