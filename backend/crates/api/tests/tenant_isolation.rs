//! Tenant-isolation integration test (PLAN.md "Done when" for M1).
//!
//! Boots the real router against a throwaway SurrealDB container and proves,
//! through the API, that a customer created under one org is invisible to
//! another. Requires a Docker-API-compatible daemon reachable via
//! `DOCKER_HOST` (see README for the local Podman setup); run with
//! `just test-int` or `cargo test -p api -- --ignored`.

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
            .expect("create request failed")
            .json()
            .await
            .expect("create response was not JSON")
    }

    async fn list_customers(&self, org_id: &str) -> Value {
        self.client
            .get(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .send()
            .await
            .expect("list request failed")
            .json()
            .await
            .expect("list response was not JSON")
    }

    async fn get_customer_status(&self, org_id: &str, id: &str) -> reqwest::StatusCode {
        self.client
            .get(format!("{}/api/customers/{id}", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .send()
            .await
            .expect("get request failed")
            .status()
    }
}

#[tokio::test]
#[ignore]
async fn customers_are_isolated_between_tenants() {
    let app = TestApp::spawn().await;

    let created = app.create_customer("org-a", "Acme Corp").await;
    let customer_id = created["id"]
        .as_str()
        .expect("created customer has an id")
        .to_string();

    let org_a_list = app.list_customers("org-a").await;
    assert_eq!(org_a_list["total"], 1);
    assert_eq!(org_a_list["items"][0]["id"], customer_id.as_str());

    let org_b_list = app.list_customers("org-b").await;
    assert_eq!(org_b_list["total"], 0);
    assert_eq!(org_b_list["items"].as_array().unwrap().len(), 0);

    let org_b_get_status = app.get_customer_status("org-b", &customer_id).await;
    assert_eq!(org_b_get_status, reqwest::StatusCode::NOT_FOUND);

    let org_a_get_status = app.get_customer_status("org-a", &customer_id).await;
    assert_eq!(org_a_get_status, reqwest::StatusCode::OK);
}
