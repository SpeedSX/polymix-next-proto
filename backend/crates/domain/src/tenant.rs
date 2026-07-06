use crate::error::DomainError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub org_id: String,
    pub db_name: String,
    pub name: String,
    pub default_language: String,
    pub default_currency: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewTenant {
    pub org_id: String,
    pub db_name: String,
    pub name: String,
}

#[async_trait::async_trait]
pub trait TenantRepo: Send + Sync {
    async fn find_by_org_id(&self, org_id: &str) -> Result<Option<Tenant>, DomainError>;
    async fn create(&self, data: NewTenant) -> Result<Tenant, DomainError>;
    /// Every provisioned tenant — used at API startup to re-run migrations
    /// against tenant databases provisioned before the latest migration was
    /// added (provisioning only runs migrations once, at creation time).
    async fn list_all(&self) -> Result<Vec<Tenant>, DomainError>;
}
