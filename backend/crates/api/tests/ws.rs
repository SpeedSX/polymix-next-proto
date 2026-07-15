//! WebSocket integration tests: auth rejection before the upgrade, change
//! envelopes for customer CRUD, and the mandatory tenant-isolation check.
//! Run with `cargo test --workspace -- --ignored` (or `just test-int`).
//!
//! Uses a real bound listener + `tokio-tungstenite` — `oneshot` can't carry
//! a WS upgrade.

mod common;

use std::time::Duration;

use common::TestApp;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

impl TestApp {
    fn ws_url(&self, token: Option<&str>) -> String {
        let base = self.base_url.replace("http://", "ws://");
        match token {
            Some(token) => format!("{base}/api/ws?token={token}"),
            None => format!("{base}/api/ws"),
        }
    }

    async fn connect_ws(&self, org_id: &str) -> WsClient {
        let token = self.token_for(org_id);
        let (client, _response) = connect_async(self.ws_url(Some(&token)))
            .await
            .expect("ws connect failed");
        client
    }

    async fn create_customer(&self, org_id: &str, name: &str) -> serde_json::Value {
        let response = self
            .client
            .post(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&serde_json::json!({ "kind": 0, "name": name, "payment_terms_days": 0, "default_discount_bp": 0 }))
            .send()
            .await
            .expect("create customer request failed");
        assert_eq!(response.status(), 201, "create customer must succeed");
        response.json().await.expect("customer body is json")
    }

    async fn update_customer(&self, org_id: &str, id: &str, name: &str) {
        let response = self
            .client
            .put(format!("{}/api/customers/{id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&serde_json::json!({ "kind": 0, "name": name, "payment_terms_days": 0, "default_discount_bp": 0 }))
            .send()
            .await
            .expect("update customer request failed");
        assert_eq!(response.status(), 200, "update customer must succeed");
    }

    async fn delete_customer(&self, org_id: &str, id: &str) {
        let response = self
            .client
            .delete(format!("{}/api/customers/{id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .send()
            .await
            .expect("delete customer request failed");
        assert_eq!(response.status(), 200, "delete customer must succeed");
    }
}

/// Next non-ping JSON frame, or panics after the timeout.
async fn next_frame(client: &mut WsClient, timeout: Duration) -> serde_json::Value {
    loop {
        let msg = tokio::time::timeout(timeout, client.next())
            .await
            .expect("timed out waiting for a ws frame")
            .expect("ws stream ended")
            .expect("ws stream errored");
        if let Message::Text(text) = msg {
            let value: serde_json::Value = serde_json::from_str(&text).expect("frame is json");
            if value["type"] == "ping" {
                let _ = client
                    .send(Message::Text(r#"{"type":"pong"}"#.into()))
                    .await;
                continue;
            }
            return value;
        }
    }
}

/// The hub's live queries are established asynchronously after the first
/// subscriber connects; a mutation racing that setup would be missed. Probe
/// with throwaway creates until one's event arrives, then drain — after
/// this, the live pipeline is warm and event order is deterministic.
async fn warm_up(app: &TestApp, client: &mut WsClient, org_id: &str) {
    for i in 0..20 {
        app.create_customer(org_id, &format!("warmup-{i}")).await;
        let arrived = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let frame = next_frame(client, Duration::from_secs(5)).await;
                if frame["type"] == "change" {
                    return;
                }
            }
        })
        .await
        .is_ok();
        if arrived {
            // Drain events from any extra probes.
            while tokio::time::timeout(Duration::from_millis(300), async {
                next_frame(client, Duration::from_secs(1)).await
            })
            .await
            .is_ok()
            {}
            return;
        }
    }
    panic!("live pipeline never delivered a warm-up event");
}

#[tokio::test]
#[ignore]
async fn missing_token_is_rejected_before_upgrade() {
    let app = TestApp::spawn().await;

    let err = connect_async(app.ws_url(None)).await.unwrap_err();

    match err {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 401);
        }
        other => panic!("expected an http 401 rejection, got {other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn invalid_token_is_rejected_before_upgrade() {
    let app = TestApp::spawn().await;

    let err = connect_async(app.ws_url(Some("not-a-jwt")))
        .await
        .unwrap_err();

    match err {
        tokio_tungstenite::tungstenite::Error::Http(response) => {
            assert_eq!(response.status(), 401);
        }
        other => panic!("expected an http 401 rejection, got {other:?}"),
    }
}

#[tokio::test]
#[ignore]
async fn customer_crud_delivers_change_envelopes_in_order() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let mut client = app.connect_ws(&org).await;
    warm_up(&app, &mut client, &org).await;

    let created = app.create_customer(&org, "Envelope Test GmbH").await;
    let id = created["id"].as_str().expect("customer has an id");

    let frame = next_frame(&mut client, Duration::from_secs(5)).await;
    assert_eq!(frame["type"], "change");
    assert_eq!(frame["entity"], "customer");
    assert_eq!(frame["action"], "create");
    assert_eq!(frame["id"], id);
    assert_eq!(frame["data"]["name"], "Envelope Test GmbH");

    app.update_customer(&org, id, "Envelope Test AG").await;
    let frame = next_frame(&mut client, Duration::from_secs(5)).await;
    assert_eq!(frame["action"], "update");
    assert_eq!(frame["id"], id);
    assert_eq!(frame["data"]["name"], "Envelope Test AG");

    app.delete_customer(&org, id).await;
    let frame = next_frame(&mut client, Duration::from_secs(5)).await;
    assert_eq!(frame["action"], "delete");
    assert_eq!(frame["id"], id);
    assert!(frame["data"].is_null(), "delete must carry data: null");
}

/// Mandatory per PLAN.md: a mutation in tenant A must reach A's client and
/// nothing may arrive at B's within the observation window.
#[tokio::test]
#[ignore]
async fn tenant_b_receives_no_events_for_tenant_a_mutations() {
    let app = TestApp::spawn().await;
    let org_a = format!("test_{}", ulid::Ulid::new());
    let org_b = format!("test_{}", ulid::Ulid::new());

    let mut client_a = app.connect_ws(&org_a).await;
    let mut client_b = app.connect_ws(&org_b).await;
    warm_up(&app, &mut client_a, &org_a).await;
    warm_up(&app, &mut client_b, &org_b).await;

    let created = app.create_customer(&org_a, "Isolation Probe").await;

    let frame = next_frame(&mut client_a, Duration::from_secs(5)).await;
    assert_eq!(frame["entity"], "customer");
    assert_eq!(frame["id"], created["id"]);

    let leaked = tokio::time::timeout(Duration::from_secs(2), async {
        next_frame(&mut client_b, Duration::from_secs(3)).await
    })
    .await;
    assert!(
        leaked.is_err(),
        "tenant B must not receive tenant A's events, got: {leaked:?}"
    );
}
