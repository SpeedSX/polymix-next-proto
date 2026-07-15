use crate::error::DomainError;
use serde::{Deserialize, Serialize};

/// Defaults for a newly provisioned tenant that didn't request otherwise
/// (the normal auth-middleware provisioning path per PLAN.md M0/M1). Seed
/// tooling (M4's Ukrainian demo tenant) passes its own values instead of
/// these — see `TenantProvisioner::provision_with_locale`.
pub const DEFAULT_LANGUAGE: &str = "en";
pub const DEFAULT_CURRENCY: &str = "EUR";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tenant {
    pub id: String,
    pub org_id: String,
    pub db_name: String,
    pub name: String,
    pub default_language: String,
    pub default_currency: String,
    /// Prefix prepended to order numbers as `"{prefix}-{NNNNNN}"`; empty
    /// means no prefix, just `"{NNNNNN}"` (PLAN.md M4: "default is empty so
    /// no prefix displayed, just number"). No admin endpoint sets this yet —
    /// out of scope for M4 — so it is always empty in practice today.
    pub order_prefix: String,
    /// Same as `order_prefix`, for invoice numbers.
    pub invoice_prefix: String,
    pub created_at: String,
    pub updated_at: String,
}

pub struct NewTenant {
    pub org_id: String,
    pub db_name: String,
    pub name: String,
    pub default_language: String,
    pub default_currency: String,
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
