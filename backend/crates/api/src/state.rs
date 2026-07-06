use std::sync::Arc;

use surreal_store::TenantProvisioner;

use crate::config::AppConfig;
use crate::dev_issuer::DevIssuer;
use crate::jwks::JwksCache;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub provisioner: Arc<TenantProvisioner>,
    pub jwks: Arc<JwksCache>,
    pub dev_issuer: Option<Arc<DevIssuer>>,
}
