use std::sync::Arc;

use surreal_store::{Store, TenantProvisioner};

use crate::config::AppConfig;
use crate::dev_issuer::DevIssuer;
use crate::jwks::JwksCache;
use crate::ws::hub::Hub;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub store: Arc<Store>,
    pub provisioner: Arc<TenantProvisioner>,
    pub jwks: Arc<JwksCache>,
    pub dev_issuer: Option<Arc<DevIssuer>>,
    pub hub: Arc<Hub>,
}
