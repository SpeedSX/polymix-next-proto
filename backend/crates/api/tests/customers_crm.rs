//! Customer CRM integration test (M5.1 "Done when", docs/customers-crm.md
//! Step 4): full CRUD round-trip of the extended customer entity through the
//! real API, status transition happy path + 409, and the order-service
//! guard (blocked customer rejected, lead auto-promoted). Run with
//! `just test-int` or `cargo test -p api -- --ignored`.

mod common;

use std::time::Duration;

use common::TestApp;
use futures::StreamExt;
use reqwest::StatusCode;
use serde_json::{Value, json};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, connect_async};

type WsClient = WebSocketStream<MaybeTlsStream<TcpStream>>;

fn full_customer_body(name: &str) -> Value {
    json!({
        "kind": 0,
        "name": name,
        "legal_name": "ТОВ «Тест»",
        "edrpou": "12345678",
        "tax_id": null,
        "vat_ipn": "123456789012",
        "tags": ["Опт ", "опт", ""],
        "industry": "Поліграфія",
        "source": "Referral",
        "website": "https://example.test",
        "contacts": [
            { "name": "Ada Lovelace", "role": "директор", "email": "ada@example.test", "phone": "+380501112233", "is_primary": true },
            { "name": "Bob Backup", "role": null, "email": null, "phone": null, "is_primary": false }
        ],
        "legal_address": { "street": "Street 1", "zip": "01001", "city": "Kyiv", "country": "UA" },
        "delivery_address": null,
        "payment_terms_days": 14,
        "credit_limit": { "amount_minor": 500000, "currency": "EUR" },
        "default_currency": "EUR",
        "default_discount_bp": 250,
        "iban": format!("UA{}", "1".repeat(27)),
        "bank_name": "PrivatBank",
        "notes": "VIP customer"
    })
}

impl TestApp {
    async fn create_customer_body(&self, org_id: &str, body: &Value) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(body)
            .send()
            .await
            .expect("create customer request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("create customer response was not JSON");
        (status, body)
    }

    async fn get_customer(&self, org_id: &str, id: &str) -> Value {
        self.client
            .get(format!("{}/api/customers/{id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .send()
            .await
            .expect("get customer request failed")
            .json()
            .await
            .expect("get customer response was not JSON")
    }

    async fn update_customer_body(
        &self,
        org_id: &str,
        id: &str,
        body: &Value,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .put(format!("{}/api/customers/{id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(body)
            .send()
            .await
            .expect("update customer request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("update customer response was not JSON");
        (status, body)
    }

    async fn update_customer_with_version(
        &self,
        org_id: &str,
        id: &str,
        body: &Value,
        expected_version: i64,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .put(format!("{}/api/customers/{id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .header(reqwest::header::IF_MATCH, expected_version.to_string())
            .json(body)
            .send()
            .await
            .expect("update customer request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("update customer response was not JSON");
        (status, body)
    }

    async fn set_customer_status(
        &self,
        org_id: &str,
        id: &str,
        status: i64,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/api/customers/{id}/status", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "status": status }))
            .send()
            .await
            .expect("set customer status request failed");
        let http_status = response.status();
        let body = response
            .json()
            .await
            .expect("set customer status response was not JSON");
        (http_status, body)
    }

    async fn create_order(&self, org_id: &str, customer_id: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/api/orders", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({
                "customer_id": customer_id,
                "currency": "EUR",
                "line_items": [
                    { "description": "Business cards", "quantity": 1, "unit_price": { "amount_minor": 100, "currency": "EUR" } }
                ],
                "notes": null
            }))
            .send()
            .await
            .expect("create order request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("create order response was not JSON");
        (status, body)
    }

    fn ws_url(&self, token: &str) -> String {
        let base = self.base_url.replace("http://", "ws://");
        format!("{base}/api/ws?token={token}")
    }

    async fn connect_ws(&self, org_id: &str) -> WsClient {
        let token = self.token_for(org_id);
        let (client, _response) = connect_async(self.ws_url(&token))
            .await
            .expect("ws connect failed");
        client
    }
}

/// Next non-ping JSON frame, or panics after the timeout — see `ws.rs`.
async fn next_frame(client: &mut WsClient, timeout: Duration) -> Value {
    loop {
        let msg = tokio::time::timeout(timeout, client.next())
            .await
            .expect("timed out waiting for a ws frame")
            .expect("ws stream ended")
            .expect("ws stream errored");
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(&text).expect("frame is json");
            if value["type"] == "ping" {
                continue;
            }
            return value;
        }
    }
}

#[tokio::test]
#[ignore]
async fn full_crud_round_trip_preserves_every_field() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let (status, created) = app
        .create_customer_body(&org, &full_customer_body("Друкарня «Аркуш»"))
        .await;
    assert_eq!(status, StatusCode::CREATED);

    assert_eq!(created["kind"], 0);
    assert_eq!(created["name"], "Друкарня «Аркуш»");
    assert_eq!(created["legal_name"], "ТОВ «Тест»");
    assert_eq!(created["edrpou"], "12345678");
    assert!(created["tax_id"].is_null());
    assert_eq!(created["vat_ipn"], "123456789012");
    // Tags are normalized: trimmed, lowercased, deduped, empties dropped.
    assert_eq!(created["tags"], json!(["опт"]));
    assert_eq!(created["industry"], "Поліграфія");
    assert_eq!(created["status"], 1, "creation defaults to active");

    let contacts = created["contacts"]
        .as_array()
        .expect("contacts is an array");
    assert_eq!(contacts.len(), 2);
    assert_eq!(contacts[0]["name"], "Ada Lovelace");
    assert_eq!(contacts[0]["is_primary"], true);
    assert_eq!(contacts[1]["is_primary"], false);

    assert_eq!(created["legal_address"]["city"], "Kyiv");
    assert!(created["delivery_address"].is_null());
    assert_eq!(created["payment_terms_days"], 14);
    assert_eq!(created["credit_limit"]["amount_minor"], 500000);
    assert_eq!(created["default_currency"], "EUR");
    assert_eq!(created["default_discount_bp"], 250);
    assert_eq!(created["bank_name"], "PrivatBank");
    assert_eq!(created["notes"], "VIP customer");

    let id = created["id"]
        .as_str()
        .expect("customer has an id")
        .to_string();
    let fetched = app.get_customer(&org, &id).await;
    assert_eq!(fetched, created);

    // PUT ignores any `status` in the body — it can only change via the
    // dedicated status route.
    let mut update_body = full_customer_body("Друкарня «Аркуш» AG");
    update_body["status"] = json!(3);
    let (status, updated) = app.update_customer_body(&org, &id, &update_body).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Друкарня «Аркуш» AG");
    assert_eq!(updated["status"], 1, "PUT must not change status");
}

#[tokio::test]
#[ignore]
async fn optimistic_concurrency_rejects_a_stale_update() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let (_, created) = app
        .create_customer_body(&org, &full_customer_body("Concurrency Co"))
        .await;
    let id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["version"], 1, "a new customer starts at version 1");

    // Two clients both loaded the record at version 1.
    let stale_version = created["version"].as_i64().unwrap();

    // First writer holds the matching version; it wins and the record advances.
    let mut first = full_customer_body("Concurrency Co");
    first["name"] = json!("First Writer Wins");
    let (status, updated) = app
        .update_customer_with_version(&org, &id, &first, stale_version)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "First Writer Wins");
    assert_eq!(
        updated["version"], 2,
        "a successful update bumps the version"
    );

    // Second writer still holds the now-stale version 1 → rejected, no clobber.
    let mut second = full_customer_body("Concurrency Co");
    second["name"] = json!("Second Writer Loses");
    let (status, body) = app
        .update_customer_with_version(&org, &id, &second, stale_version)
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "customer_modified");

    // The rejected write left no trace.
    let fetched = app.get_customer(&org, &id).await;
    assert_eq!(fetched["name"], "First Writer Wins");
    assert_eq!(fetched["version"], 2);

    // Reloading and retrying against the fresh version succeeds.
    let (status, updated) = app
        .update_customer_with_version(&org, &id, &second, 2)
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "Second Writer Loses");
    assert_eq!(updated["version"], 3);

    // Absent If-Match is backward-compatible: an unconditional write that
    // still bumps the version.
    let mut third = full_customer_body("Concurrency Co");
    third["name"] = json!("No Precondition");
    let (status, updated) = app.update_customer_body(&org, &id, &third).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["name"], "No Precondition");
    assert_eq!(updated["version"], 4);
}

#[tokio::test]
#[ignore]
async fn creation_rejects_anything_but_lead_or_active() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let mut body = full_customer_body("Invalid Status Co");
    body["status"] = json!(2);
    let (status, response) = app.create_customer_body(&org, &body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response["error"]["details"]["status"]["code"],
        "invalid_creation_status"
    );
}

#[tokio::test]
#[ignore]
async fn edrpou_is_rejected_for_a_fop() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let mut body = full_customer_body("Fop With Edrpou");
    body["kind"] = json!(1);
    body["tax_id"] = json!("1234567890");
    // edrpou from full_customer_body is only valid for kind 0.
    let (status, response) = app.create_customer_body(&org, &body).await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(
        response["error"]["details"]["edrpou"]["code"],
        "not_applicable_for_kind"
    );
}

#[tokio::test]
#[ignore]
async fn status_transitions_happy_path_and_invalid_transition() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let (_, created) = app
        .create_customer_body(&org, &full_customer_body("Status Flow Co"))
        .await;
    let id = created["id"].as_str().unwrap();
    assert_eq!(created["status"], 1);

    // active -> inactive is allowed.
    let (status, updated) = app.set_customer_status(&org, id, 2).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["status"], 2);

    // inactive -> lead is not allowed.
    let (status, body) = app.set_customer_status(&org, id, 0).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "customer_status_transition");
    assert_eq!(body["error"]["details"]["from"], "2");
    assert_eq!(body["error"]["details"]["to"], "0");
}

#[tokio::test]
#[ignore]
async fn order_creation_is_blocked_for_an_inactive_customer() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let (_, created) = app
        .create_customer_body(&org, &full_customer_body("Blocked Customer Co"))
        .await;
    let id = created["id"].as_str().unwrap();
    app.set_customer_status(&org, id, 2).await; // -> inactive

    let (status, body) = app.create_order(&org, id).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "customer_not_active_for_order");
}

#[tokio::test]
#[ignore]
async fn order_creation_for_a_lead_promotes_it_to_active() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let mut body = full_customer_body("Lead Conversion Co");
    body["status"] = json!(0);
    let (status, created) = app.create_customer_body(&org, &body).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], 0, "creation as a lead is honored");
    let id = created["id"].as_str().unwrap();

    let (status, _order) = app.create_order(&org, id).await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "a lead can place its first order"
    );

    let promoted = app.get_customer(&org, id).await;
    assert_eq!(
        promoted["status"], 1,
        "the first order promotes the lead to active"
    );
}

/// Step 4's explicit "Done when": the lead-promotion side effect of order
/// creation must reach WS subscribers like any other update, not just be
/// visible on the next `GET`.
#[tokio::test]
#[ignore]
async fn lead_promotion_emits_a_customer_update_event_over_ws() {
    let app = TestApp::spawn().await;
    let org = format!("test_{}", ulid::Ulid::new());

    let mut body = full_customer_body("WS Lead Conversion Co");
    body["status"] = json!(0);
    let (_, created) = app.create_customer_body(&org, &body).await;
    let id = created["id"].as_str().unwrap().to_string();

    let mut client = app.connect_ws(&org).await;
    // Warm up the live pipeline the same way ws.rs does, using order
    // creates (which also touch the customer table via the guard) so the
    // first genuinely-observed event isn't racing subscription setup.
    let mut warmed = false;
    for _ in 0..20 {
        let (_, warm_created) = app
            .create_customer_body(&org, &full_customer_body("warmup"))
            .await;
        let warm_id = warm_created["id"].as_str().unwrap().to_string();
        let arrived = tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let frame = next_frame(&mut client, Duration::from_secs(5)).await;
                if frame["type"] == "change" && frame["id"] == warm_id {
                    return;
                }
            }
        })
        .await
        .is_ok();
        if arrived {
            warmed = true;
            break;
        }
    }
    assert!(warmed, "live pipeline never delivered a warm-up event");

    let (status, _order) = app.create_order(&org, &id).await;
    assert_eq!(status, StatusCode::CREATED);

    // Expect a customer-update event for `id` showing the promotion, and an
    // order-create event, in either order.
    let mut saw_customer_update = false;
    let mut saw_order_create = false;
    for _ in 0..10 {
        if saw_customer_update && saw_order_create {
            break;
        }
        let frame = next_frame(&mut client, Duration::from_secs(5)).await;
        if frame["type"] != "change" {
            continue;
        }
        if frame["entity"] == "customer" && frame["id"] == id && frame["action"] == "update" {
            assert_eq!(frame["data"]["status"], 1);
            saw_customer_update = true;
        }
        if frame["entity"] == "order" && frame["action"] == "create" {
            saw_order_create = true;
        }
    }
    assert!(
        saw_customer_update,
        "expected a customer update event for the promoted lead"
    );
    assert!(saw_order_create, "expected an order create event");
}
