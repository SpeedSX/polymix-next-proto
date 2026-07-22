//! Startup retry: `Store::connect` must recover on its own from the window
//! between a SurrealDB container's port mapping and the server actually
//! serving the WS endpoint — the same race Fly.io produces when the api
//! machine starts before the SurrealDB machine does. Every other test in
//! this crate papers over this exact race with an outer retry loop around
//! `Store::connect` (see `connect_store()` in `tests/live.rs`); this test
//! instead calls it bare, immediately after the container starts, to prove
//! the retry now built into `Store::connect` itself is what's doing the
//! work.

use surreal_store::{DbConfig, Store};
use testcontainers::core::IntoContainerPort;
use testcontainers::runners::AsyncRunner;
use testcontainers::{GenericImage, ImageExt};

#[tokio::test]
#[ignore]
async fn connect_retries_through_the_container_startup_window() {
    let container = GenericImage::new("surrealdb/surrealdb", "v3.2.1")
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

    let config = DbConfig {
        url: format!("ws://127.0.0.1:{port}"),
        user: "root".to_string(),
        pass: "root".to_string(),
        ns: ulid::Ulid::new().to_string(),
    };

    Store::connect(&config)
        .await
        .expect("Store::connect should retry through the container's startup window");
}
