use std::sync::Arc;

use async_trait::async_trait;
use domain::{
    CustomerRepo, DomainError, InvoiceRepo, OrderRepo, PricingRepo, QuoteRepo, Tenant, TenantRepo,
};
use quote_engine::PriceModel;
use surreal_store::{
    Store, SurrealCustomerRepo, SurrealInvoiceRepo, SurrealOrderRepo, SurrealPricingRepo,
    SurrealQuoteRepo, SurrealTenantRepo, TenantProvisioner, migrations,
};

use crate::price_model::PriceModelCache;

#[async_trait]
pub trait Backend: Send + Sync {
    async fn customer_repo(&self, tenant_db: &str) -> Result<Arc<dyn CustomerRepo>, DomainError>;
    async fn order_repo(&self, tenant_db: &str) -> Result<Arc<dyn OrderRepo>, DomainError>;
    async fn invoice_repo(&self, tenant_db: &str) -> Result<Arc<dyn InvoiceRepo>, DomainError>;
    async fn quote_repo(&self, tenant_db: &str) -> Result<Arc<dyn QuoteRepo>, DomainError>;
    async fn pricing_repo(&self, tenant_db: &str) -> Result<Arc<dyn PricingRepo>, DomainError>;
    /// The tenant's current in-memory price-model snapshot (A2a-4), rebuilt
    /// only when the pricelist version has moved.
    async fn price_model(&self, tenant_db: &str) -> Result<Arc<PriceModel>, DomainError>;
    fn tenant_repo(&self) -> Arc<dyn TenantRepo>;
    async fn provision_tenant(&self, org_id: &str, org_name: &str) -> Result<Tenant, DomainError>;
    async fn ping(&self) -> Result<(), DomainError>;
}

/// The only API-layer adapter that knows SurrealDB's concrete repository
/// types. Routes and authentication depend only on [`Backend`].
pub struct SurrealBackend {
    store: Arc<Store>,
    provisioner: TenantProvisioner,
    price_models: PriceModelCache,
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
            price_models: PriceModelCache::new(),
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

    async fn quote_repo(&self, tenant_db: &str) -> Result<Arc<dyn QuoteRepo>, DomainError> {
        let session = self
            .store
            .for_tenant(tenant_db)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?;
        Ok(Arc::new(SurrealQuoteRepo::new(session)))
    }

    async fn pricing_repo(&self, tenant_db: &str) -> Result<Arc<dyn PricingRepo>, DomainError> {
        let session = self
            .store
            .for_tenant(tenant_db)
            .await
            .map_err(|error| DomainError::Store(error.to_string()))?;
        Ok(Arc::new(SurrealPricingRepo::new(session)))
    }

    async fn price_model(&self, tenant_db: &str) -> Result<Arc<PriceModel>, DomainError> {
        let repo = self.pricing_repo(tenant_db).await?;
        self.price_models.get(repo.as_ref(), tenant_db).await
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
