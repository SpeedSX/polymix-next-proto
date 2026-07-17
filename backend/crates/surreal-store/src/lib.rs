pub mod counter;
pub mod customer_repo;
pub mod exchange_rate;
pub mod invoice_repo;
pub mod live;
pub mod migrations;
pub mod order_repo;
pub mod provision;
mod status;
pub mod store;
pub mod tenant_repo;

pub use customer_repo::SurrealCustomerRepo;
pub use domain::{ChangeAction, ChangeEvent, LiveChange};
pub use invoice_repo::SurrealInvoiceRepo;
pub use live::live_changes;
pub use order_repo::SurrealOrderRepo;
pub use provision::TenantProvisioner;
pub use store::{DbConfig, Store};
pub use tenant_repo::SurrealTenantRepo;

/// Shared between `store` (defines the index) and `tenant_repo` (detects a
/// violation of it) so the two can't drift apart.
const TENANT_ORG_ID_INDEX: &str = "tenant_org_id";
