//! Tenant-isolation integration test (PLAN.md "Done when" for M1).
//!
//! Boots the real router against a throwaway SurrealDB container and proves,
//! through the API, that a customer created under one org is invisible to
//! another. Requires a Docker-API-compatible daemon reachable via
//! `DOCKER_HOST` (see README for the local Podman setup); run with
//! `just test-int` or `cargo test -p api -- --ignored`.

use std::time::Duration;

use api::config::AppConfig;
use api::{build_router, build_state};
use serde_json::{Value, json};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::OnceCell;

struct SharedDb {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

// One container per test binary, shared across every #[tokio::test] in this
// file — each test gets its own SurrealDB namespace instead of its own
// container, per PLAN.md.
static DB: OnceCell<SharedDb> = OnceCell::const_new();

async fn shared_db() -> &'static SharedDb {
    DB.get_or_init(|| async {
        let container = GenericImage::new("surrealdb/surrealdb", "v3.2")
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

// The container's port is mapped as soon as it's listening, but the
// SurrealDB process inside typically isn't ready to serve the RPC/WS
// endpoint for another moment — retry the first connect instead of pinning
// this to a container log line (SurrealDB's log stream isn't stable across
// versions).
async fn build_state_with_retry(config: AppConfig) -> api::state::AppState {
    let mut last_err = None;
    for _ in 0..60 {
        match build_state(config.clone()).await {
            Ok(state) => return state,
            Err(err) => {
                last_err = Some(err);
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }
    }
    panic!("failed to build app state after retrying: {last_err:?}");
}

struct TestApp {
    client: reqwest::Client,
    base_url: String,
    issuer: String,
    org_claim: String,
    dev_issuer: std::sync::Arc<api::dev_issuer::DevIssuer>,
}

impl TestApp {
    async fn spawn() -> Self {
        let db = shared_db().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("failed to bind ephemeral test listener");
        let addr = listener.local_addr().expect("listener has a local addr");
        let base_url = format!("http://{addr}");

        let config = AppConfig {
            port: addr.port(),
            surrealdb_url: db.url.clone(),
            surrealdb_user: "root".to_string(),
            surrealdb_pass: "root".to_string(),
            surrealdb_ns: ulid::Ulid::new().to_string(),
            auth_issuer: base_url.clone(),
            auth_jwks_url: format!("{base_url}/dev/jwks.json"),
            auth_org_claim: "org_id".to_string(),
            auth_audience: None,
            auth_dev_mode: true,
        };

        let state = build_state_with_retry(config).await;
        let issuer = state.config.auth_issuer.clone();
        let org_claim = state.config.auth_org_claim.clone();
        let dev_issuer = state.dev_issuer.clone().expect("dev issuer is enabled");
        let app = build_router(state);

        tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("test server failed");
        });

        TestApp {
            client: reqwest::Client::new(),
            base_url,
            issuer,
            org_claim,
            dev_issuer,
        }
    }

    fn token_for(&self, org_id: &str) -> String {
        self.dev_issuer
            .issue_token(&self.issuer, &self.org_claim, "test-user", org_id)
            .expect("failed to mint dev token")
    }

    async fn create_customer(&self, org_id: &str, name: &str) -> Value {
        self.client
            .post(format!("{}/api/customers", self.base_url))
            .bearer_auth(self.token_for(org_id))
            .json(&json!({ "name": name }))
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
