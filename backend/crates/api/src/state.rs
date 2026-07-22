use std::sync::Arc;

use crate::backend::Backend;
use crate::config::AppConfig;
use crate::dev_issuer::DevIssuer;
use crate::jwks::JwksCache;
use crate::publisher::ChangePublisher;
use crate::ws::hub::Hub;

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub backend: Arc<dyn Backend>,
    pub publisher: Arc<dyn ChangePublisher>,
    pub jwks: Arc<JwksCache>,
    pub dev_issuer: Option<Arc<DevIssuer>>,
    pub hub: Arc<Hub>,
}
