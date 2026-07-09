//! Measures whether concurrent queries on ONE shared tenant session (the
//! post-ADR-0006 session-cache behavior) serialize against each other,
//! compared to the same load spread over distinct sessions (the pre-cache
//! behavior, one session per request). All sessions share the store's
//! single WS connection either way — this isolates the per-session effect.
//!
//! Usage: SurrealDB up + `just seed` done, then
//!   cargo run --release -p surreal-store --example concurrency_probe

use std::sync::Arc;
use std::time::Instant;

use sha2::{Digest, Sha256};
use surreal_store::{DbConfig, Store};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;

const WORKERS: usize = 8;
const QUERIES_PER_WORKER: usize = 15;
const TERMS: &[&str] = &[
    "gre", "sti", "kie", "gib", "run", "bau", "kul", "abs", "mos", "lab",
];

// The heaviest single statement the split search issues — representative of
// one worker's unit of work.
const HIT_QUERY: &str = "SELECT id, name AS label, search::highlight('<b>', '</b>', 0) AS highlight, \
     search::score(0) AS score FROM customer WHERE name @0@ $q ORDER BY score DESC LIMIT 5";

fn tenant_db_name(org_id: &str) -> String {
    let digest = Sha256::digest(org_id.as_bytes());
    format!("tenant_{}", &hex::encode(digest)[..12])
}

fn percentile(sorted_ms: &[f64], pct: f64) -> f64 {
    let idx =
        ((sorted_ms.len() as f64 * pct / 100.0).ceil() as usize).clamp(1, sorted_ms.len()) - 1;
    sorted_ms[idx]
}

async fn run_query(session: &Surreal<Any>, term: &str) -> anyhow::Result<f64> {
    let started = Instant::now();
    let mut response = session
        .query(HIT_QUERY)
        .bind(("q", term.to_string()))
        .await?
        .check()?;
    let _rows: surrealdb::types::Value = response.take(0)?;
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

async fn run_workers(label: &str, sessions: Vec<Arc<Surreal<Any>>>) -> anyhow::Result<()> {
    let wall = Instant::now();
    let mut handles = Vec::new();
    for (w, session) in sessions.into_iter().enumerate() {
        handles.push(tokio::spawn(async move {
            let mut samples = Vec::with_capacity(QUERIES_PER_WORKER);
            for i in 0..QUERIES_PER_WORKER {
                let term = TERMS[(w + i) % TERMS.len()];
                samples.push(run_query(&session, term).await?);
            }
            anyhow::Ok(samples)
        }));
    }
    let mut samples = Vec::new();
    for handle in handles {
        samples.extend(handle.await??);
    }
    samples.sort_by(|a, b| a.total_cmp(b));
    let total: usize = samples.len();
    println!(
        "{label:<38} wall {:>7.0}ms  qps {:>5.0}  per-query p50 {:>6.1}ms  p95 {:>6.1}ms",
        wall.elapsed().as_secs_f64() * 1000.0,
        total as f64 / wall.elapsed().as_secs_f64(),
        percentile(&samples, 50.0),
        percentile(&samples, 95.0),
    );
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://localhost:8000".into());
    let db_name = tenant_db_name(&std::env::var("ORG_ID").unwrap_or_else(|_| "demo".into()));
    println!(
        "target: {url}, tenant db: {db_name}, {WORKERS} workers x {QUERIES_PER_WORKER} queries\n"
    );

    let store = Store::connect(&DbConfig {
        url,
        user: "root".into(),
        pass: "root".into(),
        ns: "polymix".into(),
    })
    .await?;

    // Warmup + sequential baseline on the cached session.
    let shared = store.for_tenant(&db_name).await?;
    let wall = Instant::now();
    for i in 0..WORKERS * QUERIES_PER_WORKER {
        run_query(&shared, TERMS[i % TERMS.len()]).await?;
    }
    println!(
        "{:<38} wall {:>7.0}ms",
        "sequential baseline (1 session)",
        wall.elapsed().as_secs_f64() * 1000.0
    );

    // All workers on the ONE cached session (post-change shape).
    run_workers(
        "concurrent, shared cached session",
        (0..WORKERS).map(|_| Arc::clone(&shared)).collect(),
    )
    .await?;

    // One session per worker (pre-change shape). `system()` hands out a
    // fresh first-generation clone of root each call, so `use_ns`/`use_db`
    // here reproduces the old per-request `for_tenant()` exactly without
    // going through (and hitting) the cache.
    let mut sessions = Vec::with_capacity(WORKERS);
    for _ in 0..WORKERS {
        let session = store.system();
        session.use_ns("polymix").use_db(db_name.clone()).await?;
        sessions.push(Arc::new(session));
    }
    run_workers("concurrent, session per worker", sessions).await?;

    Ok(())
}
