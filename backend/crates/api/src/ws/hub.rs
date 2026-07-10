//! Per-tenant fan-out of live-query changes to WebSocket sessions. One task
//! per tenant owns the three live queries; it starts on the first
//! subscriber and stops (dropping the live queries) on the last one's exit.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use domain::error::DomainError;
use futures::future::BoxFuture;
use futures::stream::BoxStream;
use futures::{FutureExt, StreamExt};
use serde::Serialize;
use surreal_store::{ChangeAction, ChangeEvent, LiveChange, Store, live_changes};
use tokio::sync::{Mutex, broadcast};
use tokio::task::JoinHandle;

const BROADCAST_CAPACITY: usize = 256;
const RETRY_INITIAL: Duration = Duration::from_millis(500);
const RETRY_MAX: Duration = Duration::from_secs(5);

/// The serialized WS protocol envelope (`change` | `resync`); `ping` is
/// produced per connection by the handler, not broadcast.
#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ServerEvent {
    Change {
        entity: &'static str,
        action: &'static str,
        id: String,
        data: Option<serde_json::Value>,
    },
    Resync,
}

fn envelope<T: Serialize>(entity: &'static str, event: ChangeEvent<T>) -> ServerEvent {
    ServerEvent::Change {
        entity,
        action: match event.action {
            ChangeAction::Create => "create",
            ChangeAction::Update => "update",
            ChangeAction::Delete => "delete",
        },
        id: event.id,
        data: event.data.and_then(|d| serde_json::to_value(d).ok()),
    }
}

fn to_server_event(change: LiveChange) -> ServerEvent {
    match change {
        LiveChange::Customer(event) => envelope("customer", event),
        LiveChange::Order(event) => envelope("order", event),
        LiveChange::Invoice(event) => envelope("invoice", event),
    }
}

type ChangeStream = BoxStream<'static, Result<LiveChange, DomainError>>;
type StreamFactory =
    Arc<dyn Fn(String) -> BoxFuture<'static, Result<ChangeStream, DomainError>> + Send + Sync>;

struct TenantEntry {
    tx: broadcast::Sender<Arc<ServerEvent>>,
    subscribers: usize,
    task: JoinHandle<()>,
}

pub struct Hub {
    tenants: Mutex<HashMap<String, TenantEntry>>,
    factory: StreamFactory,
}

impl Hub {
    pub fn new(store: Arc<Store>) -> Self {
        Self::with_factory(Arc::new(move |tenant_db: String| {
            let store = store.clone();
            async move {
                // A dedicated session per the tenant-session rules: live
                // queries never share a session with request traffic. The
                // stream owns the session for its lifetime (ADR 0008).
                let session = store
                    .dedicated_for_tenant(&tenant_db)
                    .await
                    .map_err(|err| DomainError::Store(err.to_string()))?;
                Ok(live_changes(session).await?.boxed())
            }
            .boxed()
        }))
    }

    fn with_factory(factory: StreamFactory) -> Self {
        Hub {
            tenants: Mutex::new(HashMap::new()),
            factory,
        }
    }

    /// Registers a subscriber for the tenant, spawning its live-query task
    /// on the 0→1 transition. The same lock guards the count and the task
    /// map, so a subscribe racing an unsubscribe either finds the live
    /// entry or creates a fresh one, never a half-dead one.
    pub async fn subscribe(&self, tenant_db: &str) -> broadcast::Receiver<Arc<ServerEvent>> {
        let mut tenants = self.tenants.lock().await;
        if let Some(entry) = tenants.get_mut(tenant_db) {
            entry.subscribers += 1;
            return entry.tx.subscribe();
        }
        let (tx, rx) = broadcast::channel(BROADCAST_CAPACITY);
        let task = tokio::spawn(tenant_task(
            self.factory.clone(),
            tenant_db.to_string(),
            tx.clone(),
        ));
        tenants.insert(
            tenant_db.to_string(),
            TenantEntry {
                tx,
                subscribers: 1,
                task,
            },
        );
        rx
    }

    /// Releases a subscriber slot; on the 1→0 transition the tenant task is
    /// aborted, which drops the live-change stream and thereby kills the
    /// live queries server-side.
    pub async fn unsubscribe(&self, tenant_db: &str) {
        let mut tenants = self.tenants.lock().await;
        let Some(entry) = tenants.get_mut(tenant_db) else {
            return;
        };
        entry.subscribers -= 1;
        if entry.subscribers == 0 {
            let entry = tenants.remove(tenant_db).expect("entry exists");
            entry.task.abort();
        }
    }
}

async fn tenant_task(
    factory: StreamFactory,
    tenant_db: String,
    tx: broadcast::Sender<Arc<ServerEvent>>,
) {
    let mut backoff = RETRY_INITIAL;
    let mut reopening = false;
    loop {
        match factory(tenant_db.clone()).await {
            Ok(mut stream) => {
                backoff = RETRY_INITIAL;
                if reopening {
                    // Clients may have missed events while the stream was
                    // down; tell them to refetch.
                    let _ = tx.send(Arc::new(ServerEvent::Resync));
                }
                reopening = true;
                loop {
                    match stream.next().await {
                        Some(Ok(change)) => {
                            let _ = tx.send(Arc::new(to_server_event(change)));
                        }
                        Some(Err(err)) => {
                            tracing::warn!(tenant_db, error = %err, "live stream error; reconnecting");
                            break;
                        }
                        None => {
                            tracing::warn!(tenant_db, "live stream ended; reconnecting");
                            break;
                        }
                    }
                }
            }
            Err(err) => {
                reopening = true;
                tracing::warn!(tenant_db, error = %err, "failed to open live stream; retrying");
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(RETRY_MAX);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    fn customer(name: &str) -> domain::customer::Customer {
        domain::customer::Customer {
            id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
            name: name.to_string(),
            contact_name: None,
            email: None,
            phone: None,
            address: None,
            notes: None,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    type ChangeSenders =
        Arc<Mutex<Vec<futures::channel::mpsc::UnboundedSender<Result<LiveChange, DomainError>>>>>;

    /// Factory that counts invocations and hands out streams fed from an
    /// mpsc channel, so tests control exactly what the tenant task sees.
    fn channel_factory(
        calls: Arc<AtomicUsize>,
        fail_first: usize,
    ) -> (StreamFactory, ChangeSenders) {
        let senders: Arc<Mutex<Vec<futures::channel::mpsc::UnboundedSender<_>>>> =
            Arc::new(Mutex::new(Vec::new()));
        let senders_out = senders.clone();
        let factory: StreamFactory = Arc::new(move |_db: String| {
            let calls = calls.clone();
            let senders = senders.clone();
            async move {
                let n = calls.fetch_add(1, Ordering::SeqCst);
                if n < fail_first {
                    return Err(DomainError::Store("factory failure".to_string()));
                }
                let (tx, rx) = futures::channel::mpsc::unbounded();
                senders.lock().await.push(tx);
                Ok(rx.boxed())
            }
            .boxed()
        });
        (factory, senders_out)
    }

    async fn wait_for_senders(senders: &ChangeSenders, count: usize) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if senders.lock().await.len() >= count {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("tenant task never opened the expected stream");
    }

    #[tokio::test]
    async fn spawns_task_on_first_subscriber_only() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (factory, senders) = channel_factory(calls.clone(), 0);
        let hub = Hub::with_factory(factory);

        let _rx1 = hub.subscribe("tenant_a").await;
        let _rx2 = hub.subscribe("tenant_a").await;
        wait_for_senders(&senders, 1).await;

        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn forwards_changes_to_subscribers() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (factory, senders) = channel_factory(calls, 0);
        let hub = Hub::with_factory(factory);

        let mut rx = hub.subscribe("tenant_a").await;
        wait_for_senders(&senders, 1).await;

        senders.lock().await[0]
            .unbounded_send(Ok(LiveChange::Customer(ChangeEvent {
                action: ChangeAction::Create,
                id: "01ARZ3NDEKTSV4RRFFQ69G5FAV".to_string(),
                data: Some(customer("Acme")),
            })))
            .unwrap();

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        match event.as_ref() {
            ServerEvent::Change {
                entity,
                action,
                id,
                data,
            } => {
                assert_eq!(*entity, "customer");
                assert_eq!(*action, "create");
                assert_eq!(id, "01ARZ3NDEKTSV4RRFFQ69G5FAV");
                assert_eq!(data.as_ref().unwrap()["name"], "Acme");
            }
            other => panic!("expected a change event, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn last_unsubscribe_aborts_task_and_next_subscribe_respawns() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (factory, senders) = channel_factory(calls.clone(), 0);
        let hub = Hub::with_factory(factory);

        let _rx = hub.subscribe("tenant_a").await;
        wait_for_senders(&senders, 1).await;
        hub.unsubscribe("tenant_a").await;
        assert!(hub.tenants.lock().await.is_empty());

        let _rx = hub.subscribe("tenant_a").await;
        wait_for_senders(&senders, 2).await;
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(start_paused = true)]
    async fn broadcasts_resync_after_reopening_a_failed_stream() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (factory, senders) = channel_factory(calls, 1);
        let hub = Hub::with_factory(factory);

        let mut rx = hub.subscribe("tenant_a").await;
        // First factory call fails; the retry (backoff auto-advanced by the
        // paused clock) succeeds and must announce a resync.
        wait_for_senders(&senders, 1).await;

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert_eq!(*event, ServerEvent::Resync);
    }

    #[tokio::test(start_paused = true)]
    async fn broadcasts_resync_when_the_stream_ends() {
        let calls = Arc::new(AtomicUsize::new(0));
        let (factory, senders) = channel_factory(calls, 0);
        let hub = Hub::with_factory(factory);

        let mut rx = hub.subscribe("tenant_a").await;
        wait_for_senders(&senders, 1).await;

        // Dropping the sender ends the stream (SurrealDB restart analogue);
        // the task must re-open and broadcast a resync.
        senders.lock().await.remove(0);
        wait_for_senders(&senders, 1).await;

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv())
            .await
            .expect("timed out")
            .expect("channel closed");
        assert_eq!(*event, ServerEvent::Resync);
    }
}
