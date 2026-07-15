//! Full-text search integration test (PLAN.md M3 "Done when"): FTS ranking
//! and the global omnibox, through the real API and a real SurrealDB. Run
//! with `just test-int` or `cargo test -p api -- --ignored`.

mod common;

use common::TestApp;
use serde_json::{Value, json};

impl TestApp {
    async fn create_customer(&self, org_id: &str, name: &str) -> Value {
        self.client
            .post(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "kind": 0, "name": name, "payment_terms_days": 0, "default_discount_bp": 0 }))
            .send()
            .await
            .expect("create customer request failed")
            .json()
            .await
            .expect("create customer response was not JSON")
    }

    async fn create_customer_with_contact(
        &self,
        org_id: &str,
        name: &str,
        contact_name: &str,
    ) -> Value {
        self.client
            .post(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({
                "kind": 0,
                "name": name,
                "payment_terms_days": 0,
                "default_discount_bp": 0,
                "contacts": [{ "name": contact_name, "role": null, "email": null, "phone": null, "is_primary": true }],
            }))
            .send()
            .await
            .expect("create customer request failed")
            .json()
            .await
            .expect("create customer response was not JSON")
    }

    async fn list_customers(&self, org_id: &str, q: &str) -> Value {
        self.client
            .get(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .query(&[("q", q)])
            .send()
            .await
            .expect("list customers request failed")
            .json()
            .await
            .expect("list customers response was not JSON")
    }

    async fn search(&self, org_id: &str, q: &str) -> Value {
        self.client
            .get(format!("{}/api/search", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .query(&[("q", q)])
            .send()
            .await
            .expect("search request failed")
            .json()
            .await
            .expect("search response was not JSON")
    }

    // Every order gets the same "Stickers" line item so tests can assert
    // that text never surfaces a match — see docs/adr/0003.
    async fn create_order(&self, org_id: &str, customer_id: &str, notes: Option<&str>) -> Value {
        self.client
            .post(format!("{}/api/orders", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({
                "customer_id": customer_id,
                "currency": "EUR",
                "line_items": [
                    { "description": "Stickers", "quantity": 1, "unit_price": { "amount_minor": 500, "currency": "EUR" } }
                ],
                "notes": notes
            }))
            .send()
            .await
            .expect("create order request failed")
            .json()
            .await
            .expect("create order response was not JSON")
    }

    async fn set_order_status(&self, org_id: &str, order_id: &str, status: i64) -> Value {
        self.client
            .post(format!("{}/api/orders/{order_id}/status", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "status": status }))
            .send()
            .await
            .expect("set order status request failed")
            .json()
            .await
            .expect("set order status response was not JSON")
    }

    async fn create_invoice_from_order(&self, org_id: &str, order_id: &str) -> Value {
        self.client
            .post(format!("{}/api/orders/{order_id}/invoice", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({}))
            .send()
            .await
            .expect("create invoice request failed")
            .json()
            .await
            .expect("create invoice response was not JSON")
    }

    async fn list_orders(&self, org_id: &str, q: &str) -> Value {
        self.client
            .get(format!("{}/api/orders", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .query(&[("q", q)])
            .send()
            .await
            .expect("list orders request failed")
            .json()
            .await
            .expect("list orders response was not JSON")
    }
}

#[tokio::test]
#[ignore]
async fn prefix_match_ranks_and_excludes_mid_word_match() {
    let app = TestApp::spawn().await;
    let org = "org-search-ranking";

    // "Adamant" starts with "ada" — an edge-ngram prefix match.
    app.create_customer(org, "Adamant Print GmbH").await;
    // "Kanada" contains "ada" mid-word, not as a token prefix. The
    // edge-ngram analyzer only ever indexes prefixes of each token, so this
    // must NOT surface — the documented "no typo tolerance" limitation
    // (PLAN.md Risks) means a fuzzier engine would find it, this one won't.
    app.create_customer(org, "Kanada Handel GmbH").await;
    // Unrelated control record.
    app.create_customer(org, "Zebra Druck AG").await;

    let body = app.list_customers(org, "ada").await;
    let items = body["items"].as_array().expect("items is an array");
    let names: Vec<&str> = items
        .iter()
        .map(|c| c["name"].as_str().expect("name is a string"))
        .collect();

    assert_eq!(names, vec!["Adamant Print GmbH"]);
    assert_eq!(body["total"], 1);
}

#[tokio::test]
#[ignore]
async fn multi_field_match_outranks_single_field_match() {
    let app = TestApp::spawn().await;
    let org = "org-search-ranking-multi";

    // Matches "ada" in the name field only.
    app.create_customer(org, "Adamant Solo GmbH").await;
    // Matches "ada" in both name and contact_name. customer_repo.rs's
    // SEARCH_SCORE sums search::score() across every matched field, and
    // BM25 term scores are never negative, so this record's combined score
    // can only be >= the single-field match's — pinning that ORDER BY score
    // DESC is load-bearing, not incidental.
    app.create_customer_with_contact(org, "Adamant Duo GmbH", "Adalina Duo")
        .await;
    // Unrelated control record.
    app.create_customer(org, "Zebra Druck AG").await;

    let body = app.list_customers(org, "ada").await;
    let items = body["items"].as_array().expect("items is an array");
    let names: Vec<&str> = items
        .iter()
        .map(|c| c["name"].as_str().expect("name is a string"))
        .collect();

    assert_eq!(names, vec!["Adamant Duo GmbH", "Adamant Solo GmbH"]);
    assert_eq!(body["total"], 2);
}

#[tokio::test]
#[ignore]
async fn omnibox_ranks_across_entities_with_highlight() {
    let app = TestApp::spawn().await;
    let org = "org-search-omnibox";

    let customer = app.create_customer(org, "Adamant Print GmbH").await;
    let customer_id = customer["id"].as_str().unwrap().to_string();
    app.create_customer(org, "Zebra Druck AG").await;

    let results = app.search(org, "adamant").await;

    let customers = results["customers"]
        .as_array()
        .expect("customers is an array");
    assert_eq!(customers.len(), 1);
    assert_eq!(customers[0]["id"], customer_id);
    assert_eq!(customers[0]["label"], "Adamant Print GmbH");
    assert_eq!(customers[0]["highlight"], "<b>Adamant</b> Print GmbH");

    assert!(results["orders"].as_array().unwrap().is_empty());
    assert!(results["invoices"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore]
async fn omnibox_handles_array_field_highlights() {
    let app = TestApp::spawn().await;
    let org = "org-search-omnibox-contact";

    let customer = app
        .create_customer_with_contact(org, "Zebra Druck AG", "Adalina Smith")
        .await;
    let customer_id = customer["id"].as_str().unwrap();

    let results = app.search(org, "adalina").await;
    let customers = results["customers"]
        .as_array()
        .expect("customers is an array");

    assert_eq!(customers.len(), 1);
    assert_eq!(customers[0]["id"], customer_id);
    assert_eq!(customers[0]["label"], "Zebra Druck AG");
    assert_eq!(customers[0]["highlight"], "<b>Adalina</b> Smith");
}

#[tokio::test]
#[ignore]
async fn order_list_search_matches_number_and_notes_but_not_line_items() {
    let app = TestApp::spawn().await;
    let org = "org-search-order-list";

    let customer = app.create_customer(org, "Adamant Print GmbH").await;
    let customer_id = customer["id"].as_str().unwrap();

    // Counters are per-tenant and start at 1 (see orders_invoices.rs), so
    // these numbers are deterministic.
    let rush_order = app
        .create_order(org, customer_id, Some("Rush job, handle with care"))
        .await;
    assert_eq!(rush_order["number"], "000001");
    let other_order = app.create_order(org, customer_id, None).await;
    assert_eq!(other_order["number"], "000002");

    let by_number = app.list_orders(org, "000001").await;
    let numbers: Vec<&str> = by_number["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|o| o["number"].as_str().unwrap())
        .collect();
    assert_eq!(numbers, vec!["000001"]);
    assert_eq!(by_number["total"], 1);

    let by_notes = app.list_orders(org, "rush").await;
    let numbers: Vec<&str> = by_notes["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|o| o["number"].as_str().unwrap())
        .collect();
    assert_eq!(numbers, vec!["000001"]);
    assert_eq!(by_notes["total"], 1);

    // ADR 0003: line_items[*].description is deliberately excluded from
    // order search, even though both orders share a "Stickers" line item
    // and the FULLTEXT index on it still exists.
    let by_line_item = app.list_orders(org, "sticker").await;
    assert_eq!(by_line_item["total"], 0);
    assert!(by_line_item["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore]
async fn order_number_search_matches_mid_number_substring() {
    let app = TestApp::spawn().await;
    let org = "org-search-order-infix";

    let customer = app.create_customer(org, "Adamant Print GmbH").await;
    let customer_id = customer["id"].as_str().unwrap();

    // First order, so its number is deterministically "000001" (per-tenant
    // counter starting at 1). Control order gets "000002".
    let order = app.create_order(org, customer_id, None).await;
    assert_eq!(order["number"], "000001");
    app.create_order(org, customer_id, None).await;

    // "0001" is a substring of "000001" starting at index 2 — not a prefix
    // (the edge-ngram prefixes of "000001" are exactly "00", "000", "0000",
    // "00000", "000001"; "0001" is none of those). Under the old
    // `autocomplete` edge-ngram analyzer this returned zero rows — the bug
    // this migration (0006_order_number_ngram) fixes. Non-anchored
    // ngram(3,10) indexes "0001" as a real 4-gram, so it must now match.
    let by_mid_substring = app.list_orders(org, "0001").await;
    let numbers: Vec<&str> = by_mid_substring["items"]
        .as_array()
        .expect("items is an array")
        .iter()
        .map(|o| o["number"].as_str().unwrap())
        .collect();
    assert_eq!(numbers, vec!["000001"]);
    assert_eq!(by_mid_substring["total"], 1);

    // ngram(3,10) has a 3-character floor: queries shorter than that
    // generate no indexed tokens, so a 2-character query must match nothing
    // (this is the min=2->3 tradeoff, traded for a smaller index).
    let by_too_short = app.list_orders(org, "00").await;
    assert_eq!(by_too_short["total"], 0);
    assert!(by_too_short["items"].as_array().unwrap().is_empty());
}

#[tokio::test]
#[ignore]
async fn omnibox_matches_order_and_invoice_hits() {
    let app = TestApp::spawn().await;
    let org = "org-search-omnibox-order-invoice";

    let customer = app.create_customer(org, "Adamant Print GmbH").await;
    let customer_id = customer["id"].as_str().unwrap();

    let order = app.create_order(org, customer_id, None).await;
    let order_id = order["id"].as_str().unwrap().to_string();
    let order_number = order["number"].as_str().unwrap().to_string();
    assert_eq!(order_number, "000001");

    app.set_order_status(org, &order_id, 1).await;
    let invoice = app.create_invoice_from_order(org, &order_id).await;
    let invoice_id = invoice["id"].as_str().unwrap().to_string();
    let invoice_number = invoice["number"].as_str().unwrap().to_string();
    assert_eq!(invoice_number, "000001");

    // "000001" matches both the order's and the invoice's number. Customers
    // have no `number` field (docs/adr/0011-drop-customer-numbering.md), so
    // there's no cross-entity collision to worry about here.
    let results = app.search(org, "000001").await;

    let customers = results["customers"]
        .as_array()
        .expect("customers is an array");
    assert!(customers.is_empty());

    let orders = results["orders"].as_array().expect("orders is an array");
    assert_eq!(orders.len(), 1);
    assert_eq!(orders[0]["id"], order_id);
    assert_eq!(orders[0]["label"], order_number);
    assert_eq!(orders[0]["highlight"], "<b>000001</b>");

    let invoices = results["invoices"]
        .as_array()
        .expect("invoices is an array");
    assert_eq!(invoices.len(), 1);
    assert_eq!(invoices[0]["id"], invoice_id);
    assert_eq!(invoices[0]["label"], invoice_number);
    assert_eq!(invoices[0]["highlight"], "<b>000001</b>");
}

#[tokio::test]
#[ignore]
async fn empty_query_returns_empty_results_without_error() {
    let app = TestApp::spawn().await;
    let org = "org-search-empty";

    let results = app.search(org, "").await;

    assert!(results["customers"].as_array().unwrap().is_empty());
    assert!(results["orders"].as_array().unwrap().is_empty());
    assert!(results["invoices"].as_array().unwrap().is_empty());
}
