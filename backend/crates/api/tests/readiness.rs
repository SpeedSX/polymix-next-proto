//! `/api/ready` must reflect real SurrealDB availability: pausing the shared
//! container flips it to 503, unpausing recovers it to 200.
//!
//! Lives in its own test binary for the same reason as `ws_resilience.rs`:
//! pausing the shared container disrupts every other test running against
//! it in parallel.

mod common;

use std::time::Duration;

use common::TestApp;

async fn ready_status(app: &TestApp) -> u16 {
    app.client
        .get(format!("{}/api/ready", app.base_url))
        .send()
        .await
        .expect("ready request failed")
        .status()
        .as_u16()
}

#[tokio::test]
#[ignore]
async fn ready_flips_with_surrealdb_availability() {
    let app = TestApp::spawn().await;

    assert_eq!(ready_status(&app).await, 200, "ready before the outage");

    let db = common::shared_db().await;
    db.pause().await;

    let mut saw_unavailable = false;
    for _ in 0..20 {
        if ready_status(&app).await == 503 {
            saw_unavailable = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert!(
        saw_unavailable,
        "ready never reported 503 while surrealdb was paused"
    );

    db.unpause().await;

    let mut recovered = false;
    for _ in 0..20 {
        if ready_status(&app).await == 200 {
            recovered = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
    assert!(
        recovered,
        "ready never recovered to 200 after unpausing surrealdb"
    );
}
