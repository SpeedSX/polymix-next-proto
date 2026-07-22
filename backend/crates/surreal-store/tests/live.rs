//! Integration tests for the typed live-change streams (`src/live.rs`),
//! against a real SurrealDB via testcontainers. Run with
//! `cargo test --workspace -- --ignored` (or `just test-int`).

use std::sync::Arc;
use std::time::Duration;

use domain::customer::{CustomerRepo, NewCustomer};
use futures::{Stream, StreamExt};
use surreal_store::{
    ChangeAction, DbConfig, LiveChange, Store, SurrealCustomerRepo, TenantProvisioner, live_changes,
};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::OnceCell;

struct SharedDb {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

static DB: OnceCell<SharedDb> = OnceCell::const_new();

async fn shared_db() -> &'static SharedDb {
    DB.get_or_init(|| async {
        let container = GenericImage::new("surrealdb/surrealdb", "v3.2.3")
            .with_exposed_port(8000.tcp())
            .with_cmd([
                "start",
                "--user",
                "root",
                "--pass",
                "root",
                "--bind",
                "0.0.0.0:8000",
                "memory",
            ])
            .start()
            .await
            .expect("failed to start surrealdb container");
        let port = container
            .get_host_port_ipv4(8000)
            .await
            .expect("failed to read mapped surrealdb port");
        SharedDb {
            _container: container,
            url: format!("ws://127.0.0.1:{port}"),
        }
    })
    .await
}

// The container's port maps before SurrealDB serves the WS endpoint;
// retry the first connect (same rationale as the api harness).
async fn connect_with_config(config: &DbConfig) -> Arc<Store> {
    let mut last_err = None;
    for _ in 0..60 {
        match Store::connect(config).await {
            Ok(store) => return Arc::new(store),
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    panic!("failed to connect store after retrying: {last_err:?}");
}

async fn connect_store() -> Arc<Store> {
    let db = shared_db().await;
    let config = DbConfig {
        url: db.url.clone(),
        user: "root".to_string(),
        pass: "root".to_string(),
        ns: ulid::Ulid::new().to_string(),
    };
    connect_with_config(&config).await
}

fn new_customer(name: &str) -> NewCustomer {
    NewCustomer {
        kind: domain::customer::CustomerKind::LegalEntity,
        name: name.to_string(),
        legal_name: None,
        edrpou: None,
        tax_id: None,
        vat_ipn: None,
        tags: vec![],
        industry: None,
        source: None,
        website: None,
        contacts: vec![],
        legal_address: None,
        delivery_address: None,
        payment_terms_days: 0,
        credit_limit: None,
        default_currency: Some("EUR".to_string()),
        default_discount_bp: 0,
        iban: None,
        bank_name: None,
        notes: None,
        status: None,
    }
}

async fn next_change(
    stream: &mut (impl Stream<Item = Result<LiveChange, domain::error::DomainError>> + Unpin),
) -> LiveChange {
    tokio::time::timeout(Duration::from_secs(10), stream.next())
        .await
        .expect("timed out waiting for a live change")
        .expect("live stream ended unexpectedly")
        .expect("live stream yielded an error")
}

#[tokio::test]
#[ignore]
async fn delivers_customer_create_update_delete() {
    let store = connect_store().await;
    let provisioner = TenantProvisioner::new(store.clone());
    let tenant = provisioner
        .ensure_tenant(&format!("test_{}", ulid::Ulid::new()), "Live Test")
        .await
        .expect("tenant provisioning failed");
    let session = store.for_tenant(&tenant.db_name).await.unwrap();

    let mut stream = live_changes(session.clone()).await.unwrap();
    let repo = SurrealCustomerRepo::new(session);

    let created = repo
        .create(new_customer("Adamant Print GmbH"), &tenant)
        .await
        .unwrap();
    match next_change(&mut stream).await {
        LiveChange::Customer(event) => {
            assert_eq!(event.action, ChangeAction::Create);
            assert_eq!(
                event.id, created.id,
                "id must be the key part, not `customer:…`"
            );
            let data = event.data.expect("create carries the entity");
            assert_eq!(data.name, "Adamant Print GmbH");
        }
        other => panic!("expected a customer change, got {other:?}"),
    }

    repo.update(&created.id, new_customer("Adamant Print AG"), None, &tenant)
        .await
        .unwrap();
    match next_change(&mut stream).await {
        LiveChange::Customer(event) => {
            assert_eq!(event.action, ChangeAction::Update);
            assert_eq!(event.id, created.id);
            assert_eq!(
                event.data.expect("update carries the entity").name,
                "Adamant Print AG"
            );
        }
        other => panic!("expected a customer change, got {other:?}"),
    }

    repo.delete(&created.id).await.unwrap();
    match next_change(&mut stream).await {
        LiveChange::Customer(event) => {
            assert_eq!(event.action, ChangeAction::Delete);
            assert_eq!(event.id, created.id);
            assert!(event.data.is_none(), "delete must carry data: none");
        }
        other => panic!("expected a customer change, got {other:?}"),
    }
}

/// The hub's exact wiring: live queries on a `dedicated_for_tenant` session
/// while writes come through the cached `for_tenant` session used by request
/// handlers. Guards the cross-session delivery the WS pipeline depends on.
#[tokio::test]
#[ignore]
async fn delivers_across_sessions() {
    let store = connect_store().await;
    let provisioner = TenantProvisioner::new(store.clone());
    let tenant = provisioner
        .ensure_tenant(&format!("test_{}", ulid::Ulid::new()), "Cross Session")
        .await
        .expect("tenant provisioning failed");

    let live_session = store.dedicated_for_tenant(&tenant.db_name).await.unwrap();
    let mut stream = live_changes(live_session).await.unwrap();

    let write_session = store.for_tenant(&tenant.db_name).await.unwrap();
    let repo = SurrealCustomerRepo::new(write_session);
    let created = repo
        .create(new_customer("Cross Session GmbH"), &tenant)
        .await
        .unwrap();

    match next_change(&mut stream).await {
        LiveChange::Customer(event) => {
            assert_eq!(event.action, ChangeAction::Create);
            assert_eq!(event.id, created.id);
        }
        other => panic!("expected a customer change, got {other:?}"),
    }
}

/// Same shape but the write comes over a completely separate connection —
/// distinguishes an SDK session-multiplexing gap from a server-side
/// limitation of live-query delivery.
#[tokio::test]
#[ignore]
async fn delivers_across_connections() {
    let db = shared_db().await;
    let ns = ulid::Ulid::new().to_string();
    let config = DbConfig {
        url: db.url.clone(),
        user: "root".to_string(),
        pass: "root".to_string(),
        ns,
    };
    let store_a = connect_with_config(&config).await;
    let store_b = connect_with_config(&config).await;

    let provisioner = TenantProvisioner::new(store_a.clone());
    let tenant = provisioner
        .ensure_tenant(&format!("test_{}", ulid::Ulid::new()), "Cross Conn")
        .await
        .expect("tenant provisioning failed");

    let live_session = store_a.for_tenant(&tenant.db_name).await.unwrap();
    let mut stream = live_changes(live_session).await.unwrap();

    let write_session = store_b.for_tenant(&tenant.db_name).await.unwrap();
    let repo = SurrealCustomerRepo::new(write_session);
    let created = repo
        .create(new_customer("Cross Conn GmbH"), &tenant)
        .await
        .unwrap();

    match next_change(&mut stream).await {
        LiveChange::Customer(event) => {
            assert_eq!(event.action, ChangeAction::Create);
            assert_eq!(event.id, created.id);
        }
        other => panic!("expected a customer change, got {other:?}"),
    }
}

/// Dropping the stream must kill the live queries server-side (the SDK
/// sends `KILL` from the inner streams' `Drop`; the hub's teardown relies on
/// it). Observed indirectly: after the drop the session keeps working and a
/// freshly opened stream sees only mutations made after it was opened.
#[tokio::test]
#[ignore]
async fn dropping_the_stream_releases_the_session() {
    let store = connect_store().await;
    let provisioner = TenantProvisioner::new(store.clone());
    let tenant = provisioner
        .ensure_tenant(&format!("test_{}", ulid::Ulid::new()), "Live Drop Test")
        .await
        .expect("tenant provisioning failed");
    let session = store.for_tenant(&tenant.db_name).await.unwrap();
    let repo = SurrealCustomerRepo::new(session.clone());

    let stream = live_changes(session.clone()).await.unwrap();
    drop(stream);

    repo.create(new_customer("Made While No Stream"), &tenant)
        .await
        .unwrap();

    let mut stream = live_changes(session.clone()).await.unwrap();
    let created = repo
        .create(new_customer("Made While Streaming"), &tenant)
        .await
        .unwrap();
    match next_change(&mut stream).await {
        LiveChange::Customer(event) => {
            assert_eq!(event.action, ChangeAction::Create);
            assert_eq!(event.id, created.id);
        }
        other => panic!("expected a customer change, got {other:?}"),
    }
}
