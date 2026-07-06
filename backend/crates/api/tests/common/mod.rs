//! Shared integration-test harness: one SurrealDB testcontainer per test
//! binary (see PLAN.md's "Integration test harness + CI") plus a thin HTTP
//! client wrapper around the real router. Domain-specific request helpers
//! (e.g. `create_customer`, `create_order`) live in each test file as
//! additional `impl TestApp` blocks — every `tests/*.rs` file is its own
//! crate, so there's no conflict between them.

use std::time::Duration;

use api::config::AppConfig;
use api::{build_router, build_state};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{ContainerAsync, GenericImage, ImageExt};
use tokio::sync::OnceCell;

pub struct SharedDb {
    _container: ContainerAsync<GenericImage>,
    url: String,
}

static DB: OnceCell<SharedDb> = OnceCell::const_new();

pub async fn shared_db() -> &'static SharedDb {
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

pub struct TestApp {
    pub client: reqwest::Client,
    pub base_url: String,
    pub issuer: String,
    pub org_claim: String,
    pub dev_issuer: std::sync::Arc<api::dev_issuer::DevIssuer>,
}

impl TestApp {
    pub async fn spawn() -> Self {
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

    pub fn token_for(&self, org_id: &str) -> String {
        self.dev_issuer
            .issue_token(&self.issuer, &self.org_claim, "test-user", org_id)
            .expect("failed to mint dev token")
    }
}
