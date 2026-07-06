pub mod customer_repo;
pub mod migrations;
pub mod provision;
pub mod store;
pub mod tenant_repo;

pub use customer_repo::SurrealCustomerRepo;
pub use provision::TenantProvisioner;
pub use store::{DbConfig, Store};
pub use tenant_repo::SurrealTenantRepo;

/// Shared between `store` (defines the index) and `tenant_repo` (detects a
/// violation of it) so the two can't drift apart.
const TENANT_ORG_ID_INDEX: &str = "tenant_org_id";
