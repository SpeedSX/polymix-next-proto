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
            .json(&json!({ "name": name }))
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
async fn empty_query_returns_empty_results_without_error() {
    let app = TestApp::spawn().await;
    let org = "org-search-empty";

    let results = app.search(org, "").await;

    assert!(results["customers"].as_array().unwrap().is_empty());
    assert!(results["orders"].as_array().unwrap().is_empty());
    assert!(results["invoices"].as_array().unwrap().is_empty());
}
