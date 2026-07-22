use domain::LiveChange;

pub trait ChangePublisher: Send + Sync {
    fn publish(&self, tenant_db: &str, change: LiveChange);
}

/// SurrealDB live queries already feed the WebSocket hub, so handler-side
/// publication must be disabled to avoid duplicate events.
pub struct NoopPublisher;

impl ChangePublisher for NoopPublisher {
    fn publish(&self, _tenant_db: &str, _change: LiveChange) {}
}
