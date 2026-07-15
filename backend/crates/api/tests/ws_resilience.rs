//! WS resilience: pausing and unpausing SurrealDB must not leave connected
//! clients with a dead live pipeline — after recovery a subsequent
//! mutation's event still arrives (preceded by a `resync` when the hub had
//! to re-open its streams).
//!
//! Lives in its own test binary: pausing the shared container would disrupt
//! any test running in parallel against it.

mod common;

use std::time::Duration;

use common::TestApp;
use futures::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

async fn create_customer(app: &TestApp, org_id: &str, name: &str) -> serde_json::Value {
    let response = app
        .client
        .post(format!("{}/api/customers", app.base_url))
        .bearer_auth(app.token_for(org_id))
        .json(&serde_json::json!({ "kind": 0, "name": name, "payment_terms_days": 0, "default_discount_bp": 0 }))
        .send()
        .await
        .expect("create customer request failed");
    assert_eq!(response.status(), 201, "create customer must succeed");
    response.json().await.expect("customer body is json")
}

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

#[tokio::test]
#[ignore]
async fn live_updates_recover_after_surrealdb_pause() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());
    let token = app.token_for(&org);

    let ws_url = format!(
        "{}/api/ws?token={token}",
        app.base_url.replace("http://", "ws://")
    );
    let (mut client, _) = connect_async(ws_url).await.expect("ws connect failed");

    // Warm up: prove the pipeline delivers before the outage.
    let mut warm = false;
    for i in 0..20 {
        create_customer(&app, &org, &format!("warmup-{i}")).await;
        if tokio::time::timeout(
            Duration::from_secs(1),
            next_frame(&mut client, Duration::from_secs(2)),
        )
        .await
        .is_ok()
        {
            warm = true;
            break;
        }
    }
    assert!(warm, "live pipeline never delivered before the pause");
    while tokio::time::timeout(
        Duration::from_millis(300),
        next_frame(&mut client, Duration::from_secs(1)),
    )
    .await
    .is_ok()
    {}

    // Outage: freeze SurrealDB long enough for the SDK connection (and with
    // it the live streams) to die, then thaw.
    let db = common::shared_db().await;
    db.pause().await;
    tokio::time::sleep(Duration::from_secs(12)).await;
    db.unpause().await;

    // Recovery: keep mutating until an event arrives again. REST mutations
    // may themselves fail right after the thaw (the request session is
    // reconnecting too) — retry those as well. A `resync` frame may arrive
    // first if the hub re-opened its streams; both prove recovery.
    let mut recovered = false;
    for i in 0..60 {
        let response = app
            .client
            .post(format!("{}/api/customers", app.base_url))
            .bearer_auth(&token)
            .json(&serde_json::json!({
                "kind": 0,
                "name": format!("post-outage-{i}"),
                "payment_terms_days": 0,
                "default_discount_bp": 0,
            }))
            .send()
            .await;
        let created_ok = matches!(&response, Ok(r) if r.status() == 201);

        if let Ok(frame) = tokio::time::timeout(
            Duration::from_secs(1),
            next_frame(&mut client, Duration::from_secs(2)),
        )
        .await
        {
            assert!(
                frame["type"] == "change" || frame["type"] == "resync",
                "unexpected frame after recovery: {frame}"
            );
            if frame["type"] == "change" {
                recovered = true;
                break;
            }
        }
        if !created_ok {
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
    }
    assert!(
        recovered,
        "no change event arrived after the surrealdb pause/unpause"
    );
}
