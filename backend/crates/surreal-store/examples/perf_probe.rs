//! Decomposes /api/search's customer-leg latency into its phases against
//! the seeded demo tenant, to decide where optimization effort goes
//! (session-open vs. query cost vs. count vs. individual FULLTEXT fields).
//! See docs/adr/0004-search-p95-exceeds-target.md for the numbers this is
//! meant to explain.
//!
//! Usage: SurrealDB up + `just seed` done, then
//!   cargo run --release -p surreal-store --example perf_probe

use std::time::Instant;

use sha2::{Digest, Sha256};
use surreal_store::{DbConfig, Store};
use surrealdb::Surreal;
use surrealdb::engine::any::Any;

const ITERATIONS: usize = 40;
const WARMUP: usize = 3;
const TERMS: &[&str] = &[
    "gre", "sti", "kie", "gib", "run", "bau", "kul", "abs", "mos", "lab",
];

const CUSTOMER_HIT_QUERY: &str = "SELECT id, name AS label, search::highlight('<b>', '</b>', 0) AS highlight, \
     (search::score(0) + search::score(1) + search::score(2) + search::score(3)) AS score \
     FROM customer \
     WHERE name @0@ $q OR contact_name @1@ $q OR email @2@ $q OR address.city @3@ $q \
     ORDER BY score DESC LIMIT 5";

const CUSTOMER_LIST_QUERY: &str = "SELECT *, (search::score(0) + search::score(1) + search::score(2) + search::score(3)) AS score \
     FROM customer \
     WHERE name @0@ $q OR contact_name @1@ $q OR email @2@ $q OR address.city @3@ $q \
     ORDER BY score DESC LIMIT 25 START 0";

const CUSTOMER_COUNT_QUERY: &str = "SELECT count() FROM (SELECT id FROM customer \
     WHERE name @0@ $q OR contact_name @1@ $q OR email @2@ $q OR address.city @3@ $q) GROUP ALL";

const ORDER_HIT_QUERY: &str = "SELECT id, number AS label, search::highlight('<b>', '</b>', 0) AS highlight, \
     (search::score(0) + search::score(1)) AS score \
     FROM `order` WHERE number @0@ $q OR notes @1@ $q \
     ORDER BY score DESC LIMIT 5";

fn single_field_query(field: &str) -> String {
    format!(
        "SELECT id, name AS label, search::score(0) AS score \
         FROM customer WHERE {field} @0@ $q ORDER BY score DESC LIMIT 5"
    )
}

fn tenant_db_name(org_id: &str) -> String {
    let digest = Sha256::digest(org_id.as_bytes());
    format!("tenant_{}", &hex::encode(digest)[..12])
}

fn percentile(sorted_ms: &[f64], pct: f64) -> f64 {
    let idx =
        ((sorted_ms.len() as f64 * pct / 100.0).ceil() as usize).clamp(1, sorted_ms.len()) - 1;
    sorted_ms[idx]
}

fn report(label: &str, mut samples: Vec<f64>) {
    samples.sort_by(|a, b| a.total_cmp(b));
    println!(
        "{label:<44} p50 {:>7.1}ms  p95 {:>7.1}ms  max {:>7.1}ms",
        percentile(&samples, 50.0),
        percentile(&samples, 95.0),
        samples.last().copied().unwrap_or(0.0),
    );
}

async fn time_query(session: &Surreal<Any>, sql: &str, term: &str) -> anyhow::Result<f64> {
    let started = Instant::now();
    let mut response = session
        .query(sql)
        .bind(("q", term.to_string()))
        .await?
        .check()?;
    // Force full deserialization so wire + decode cost is included.
    let _rows: surrealdb::types::Value = response.take(0)?;
    Ok(started.elapsed().as_secs_f64() * 1000.0)
}

async fn bench_query(label: &str, session: &Surreal<Any>, sql: &str) -> anyhow::Result<()> {
    let mut samples = Vec::with_capacity(ITERATIONS);
    for i in 0..ITERATIONS + WARMUP {
        let term = TERMS[i % TERMS.len()];
        let ms = time_query(session, sql, term).await?;
        if i >= WARMUP {
            samples.push(ms);
        }
    }
    report(label, samples);
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let url = std::env::var("SURREALDB_URL").unwrap_or_else(|_| "ws://localhost:8000".into());
    let org_id = std::env::var("ORG_ID").unwrap_or_else(|_| "demo".into());
    let db_name = tenant_db_name(&org_id);
    println!("target: {url}, tenant db: {db_name}\n");

    let store = Store::connect(&DbConfig {
        url,
        user: "root".into(),
        pass: "root".into(),
        ns: "polymix".into(),
    })
    .await?;

    // Phase A: session-open cost, exactly what every handler pays per request.
    let mut samples = Vec::with_capacity(ITERATIONS);
    for i in 0..ITERATIONS + WARMUP {
        let started = Instant::now();
        let _session = store.for_tenant(&db_name).await?;
        if i >= WARMUP {
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    report("A: for_tenant() session open", samples);

    // Phases B..G: query cost on one warm session, no per-iteration setup.
    let warm = store.for_tenant(&db_name).await?;
    bench_query(
        "B: customer omnibox hit query (warm)",
        &warm,
        CUSTOMER_HIT_QUERY,
    )
    .await?;
    bench_query(
        "C: customer list query, 25 rows (warm)",
        &warm,
        CUSTOMER_LIST_QUERY,
    )
    .await?;
    bench_query(
        "D: customer count subquery (warm)",
        &warm,
        CUSTOMER_COUNT_QUERY,
    )
    .await?;
    for field in ["name", "contact_name", "email", "address.city"] {
        bench_query(
            &format!("E: single field: {field} (warm)"),
            &warm,
            &single_field_query(field),
        )
        .await?;
    }
    bench_query("F: order omnibox hit query (warm)", &warm, ORDER_HIT_QUERY).await?;

    // Phase G: full production shape — fresh session + hit query, like a
    // real /api/search customer leg.
    let mut samples = Vec::with_capacity(ITERATIONS);
    for i in 0..ITERATIONS + WARMUP {
        let term = TERMS[i % TERMS.len()];
        let started = Instant::now();
        let session = store.for_tenant(&db_name).await?;
        let mut response = session
            .query(CUSTOMER_HIT_QUERY)
            .bind(("q", term.to_string()))
            .await?
            .check()?;
        let _rows: surrealdb::types::Value = response.take(0)?;
        if i >= WARMUP {
            samples.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    report("G: fresh session + hit query (prod shape)", samples);

    Ok(())
}
