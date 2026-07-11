//! Orders + invoices integration test (PLAN.md "Done when" for M2): the full
//! create -> confirm -> invoice -> issue flow through the real API, plus the
//! conflict cases the milestone calls out explicitly (invalid transitions,
//! double invoicing, delete blocked by references). Run with `just test-int`
//! or `cargo test -p api -- --ignored`.

mod common;

use common::TestApp;
use reqwest::StatusCode;
use serde_json::{Value, json};

impl TestApp {
    async fn create_customer(&self, org_id: &str, name: &str) -> Value {
        self.client
            .post(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "name": name }))
            .send()
            .await
            .expect("create customer request failed")
            .json()
            .await
            .expect("create customer response was not JSON")
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
                    { "description": "Business cards", "quantity": 3, "unit_price": { "amount_minor": 250, "currency": "EUR" } },
                    { "description": "Flyers", "quantity": 2, "unit_price": { "amount_minor": 1000, "currency": "EUR" } }
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

    async fn set_order_status(
        &self,
        org_id: &str,
        order_id: &str,
        status: &str,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/api/orders/{order_id}/status", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "status": status }))
            .send()
            .await
            .expect("set order status request failed");
        let http_status = response.status();
        let body = response
            .json()
            .await
            .expect("set order status response was not JSON");
        (http_status, body)
    }

    async fn create_invoice_from_order(&self, org_id: &str, order_id: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!("{}/api/orders/{order_id}/invoice", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({}))
            .send()
            .await
            .expect("create invoice request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("create invoice response was not JSON");
        (status, body)
    }

    async fn set_invoice_status(
        &self,
        org_id: &str,
        invoice_id: &str,
        status: &str,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .post(format!(
                "{}/api/invoices/{invoice_id}/status",
                self.base_url
            ))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "status": status }))
            .send()
            .await
            .expect("set invoice status request failed");
        let http_status = response.status();
        let body = response
            .json()
            .await
            .expect("set invoice status response was not JSON");
        (http_status, body)
    }

    async fn update_invoice(
        &self,
        org_id: &str,
        invoice_id: &str,
        line_items: Value,
    ) -> (StatusCode, Value) {
        let response = self
            .client
            .put(format!("{}/api/invoices/{invoice_id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "line_items": line_items }))
            .send()
            .await
            .expect("update invoice request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("update invoice response was not JSON");
        (status, body)
    }

    async fn delete_customer(&self, org_id: &str, customer_id: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .delete(format!("{}/api/customers/{customer_id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .send()
            .await
            .expect("delete customer request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("delete customer response was not JSON");
        (status, body)
    }

    async fn delete_order(&self, org_id: &str, order_id: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .delete(format!("{}/api/orders/{order_id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .send()
            .await
            .expect("delete order request failed");
        let status = response.status();
        let body = response
            .json()
            .await
            .expect("delete order response was not JSON");
        (status, body)
    }
}

#[tokio::test]
#[ignore]
async fn full_order_to_paid_invoice_flow() {
    let app = TestApp::spawn().await;
    let org = "org-flow";

    let customer = app.create_customer(org, "Adamant Print GmbH").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();

    let (status, order) = app.create_order(org, &customer_id).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(order["status"], "draft");
    assert_eq!(order["number"], "000001");
    // 3 * 250 + 2 * 1000 = 2750
    assert_eq!(order["total"]["amount_minor"], 2750);
    assert_eq!(order["total"]["currency"], "EUR");
    let order_id = order["id"].as_str().unwrap().to_string();

    let (status, order) = app.set_order_status(org, &order_id, "confirmed").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(order["status"], "confirmed");

    let (status, invoice) = app.create_invoice_from_order(org, &order_id).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(invoice["status"], "draft");
    assert_eq!(invoice["number"], "000001");
    assert_eq!(invoice["net_total"]["amount_minor"], 2750);
    assert_eq!(invoice["tax_rate_bp"], 1900);
    // round(2750 * 1900 / 10000) = round(522.5) = 523 (half-up)
    assert_eq!(invoice["tax_total"]["amount_minor"], 523);
    assert_eq!(invoice["gross_total"]["amount_minor"], 2750 + 523);
    let invoice_id = invoice["id"].as_str().unwrap().to_string();

    let (status, invoice) = app.set_invoice_status(org, &invoice_id, "issued").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invoice["status"], "issued");
    assert!(invoice["issue_date"].is_string());
    assert!(invoice["due_date"].is_string());

    let (status, invoice) = app.set_invoice_status(org, &invoice_id, "paid").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(invoice["status"], "paid");
}

#[tokio::test]
#[ignore]
async fn invalid_order_transition_is_rejected() {
    let app = TestApp::spawn().await;
    let org = "org-invalid-transition";

    let customer = app.create_customer(org, "Acme").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    let (_, order) = app.create_order(org, &customer_id).await;
    let order_id = order["id"].as_str().unwrap().to_string();

    // draft -> in_production skips confirmed; must be rejected.
    let (status, body) = app.set_order_status(org, &order_id, "in_production").await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "order_status_transition");
    assert_eq!(body["error"]["details"]["from"], "draft");
    assert_eq!(body["error"]["details"]["to"], "in_production");
}

#[tokio::test]
#[ignore]
async fn ordering_an_invoice_twice_is_rejected() {
    let app = TestApp::spawn().await;
    let org = "org-double-invoice";

    let customer = app.create_customer(org, "Acme").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    let (_, order) = app.create_order(org, &customer_id).await;
    let order_id = order["id"].as_str().unwrap().to_string();
    app.set_order_status(org, &order_id, "confirmed").await;

    let (first_status, _) = app.create_invoice_from_order(org, &order_id).await;
    assert_eq!(first_status, StatusCode::CREATED);

    let (second_status, second_body) = app.create_invoice_from_order(org, &order_id).await;
    assert_eq!(second_status, StatusCode::CONFLICT);
    assert_eq!(second_body["error"]["code"], "order_already_invoiced");
}

#[tokio::test]
#[ignore]
async fn uninvoiceable_order_status_is_rejected() {
    let app = TestApp::spawn().await;
    let org = "org-draft-invoice";

    let customer = app.create_customer(org, "Acme").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    let (_, order) = app.create_order(org, &customer_id).await;
    let order_id = order["id"].as_str().unwrap().to_string();

    // Order is still "draft" — invoicing must be rejected.
    let (status, body) = app.create_invoice_from_order(org, &order_id).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "order_not_confirmed_for_invoice");
}

#[tokio::test]
#[ignore]
async fn deletes_are_blocked_by_references() {
    let app = TestApp::spawn().await;
    let org = "org-delete-guard";

    let customer = app.create_customer(org, "Acme").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    let (_, order) = app.create_order(org, &customer_id).await;
    let order_id = order["id"].as_str().unwrap().to_string();

    // Customer has an order -> delete blocked.
    let (customer_delete_status, customer_delete_body) =
        app.delete_customer(org, &customer_id).await;
    assert_eq!(customer_delete_status, StatusCode::CONFLICT);
    assert_eq!(customer_delete_body["error"]["code"], "customer_has_orders");

    app.set_order_status(org, &order_id, "confirmed").await;
    let (invoice_status, invoice) = app.create_invoice_from_order(org, &order_id).await;
    assert_eq!(invoice_status, StatusCode::CREATED);
    let _ = invoice;

    // Order now has an invoice -> delete blocked.
    let (order_delete_status, order_delete_body) = app.delete_order(org, &order_id).await;
    assert_eq!(order_delete_status, StatusCode::CONFLICT);
    assert_eq!(order_delete_body["error"]["code"], "order_has_invoice");
}

#[tokio::test]
#[ignore]
async fn draft_invoice_put_recomputes_totals() {
    let app = TestApp::spawn().await;
    let org = "org-edit-draft-invoice";

    let customer = app.create_customer(org, "Acme").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    let (_, order) = app.create_order(org, &customer_id).await;
    let order_id = order["id"].as_str().unwrap().to_string();
    app.set_order_status(org, &order_id, "confirmed").await;

    let (_, invoice) = app.create_invoice_from_order(org, &order_id).await;
    assert_eq!(invoice["status"], "draft");
    // 3 * 250 + 2 * 1000 = 2750, per create_order's fixed line items.
    assert_eq!(invoice["net_total"]["amount_minor"], 2750);
    let invoice_id = invoice["id"].as_str().unwrap().to_string();

    let (status, updated) = app
        .update_invoice(
            org,
            &invoice_id,
            json!([
                { "description": "Business cards", "quantity": 10, "unit_price": { "amount_minor": 250, "currency": "EUR" } }
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["line_items"].as_array().unwrap().len(), 1);
    // 10 * 250 = 2500
    assert_eq!(updated["net_total"]["amount_minor"], 2500);
    // round(2500 * 1900 / 10000) = 475
    assert_eq!(updated["tax_total"]["amount_minor"], 475);
    assert_eq!(updated["gross_total"]["amount_minor"], 2500 + 475);
}

#[tokio::test]
#[ignore]
async fn issued_invoice_put_is_rejected() {
    let app = TestApp::spawn().await;
    let org = "org-edit-issued-invoice";

    let customer = app.create_customer(org, "Acme").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    let (_, order) = app.create_order(org, &customer_id).await;
    let order_id = order["id"].as_str().unwrap().to_string();
    app.set_order_status(org, &order_id, "confirmed").await;

    let (_, invoice) = app.create_invoice_from_order(org, &order_id).await;
    let invoice_id = invoice["id"].as_str().unwrap().to_string();
    app.set_invoice_status(org, &invoice_id, "issued").await;

    let (status, body) = app
        .update_invoice(
            org,
            &invoice_id,
            json!([
                { "description": "Business cards", "quantity": 1, "unit_price": { "amount_minor": 250, "currency": "EUR" } }
            ]),
        )
        .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "invoice_not_draft");
}
