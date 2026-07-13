use std::sync::Arc;
use std::time::{Duration, Instant};

use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Root;

use crate::TENANT_ORG_ID_INDEX;

pub struct DbConfig {
    pub url: String,
    pub user: String,
    pub pass: String,
    pub ns: String,
}

const SYSTEM_DB: &str = "system";

// Startup retry for `Store::connect`: on Fly.io the api container can start
// before the SurrealDB machine accepts connections, and without this the
// process would exit and rely on the orchestrator to restart it repeatedly
// until the race resolves itself.
const CONNECT_INITIAL_BACKOFF: Duration = Duration::from_millis(500);
const CONNECT_MAX_BACKOFF: Duration = Duration::from_secs(5);
const CONNECT_DEADLINE: Duration = Duration::from_secs(30);

/// Root-authenticated session pinned to the `system` db. Never handed out
/// directly — tenant sessions are cloned from it (SDK >= 3.0: a clone gets
/// its own independent namespace/database selection on the same underlying
/// connection, so concurrent tenant sessions never affect each other).
pub struct Store {
    root: Surreal<Any>,
    ns: String,
    // Session setup (`root.clone()` + `use_ns`/`use_db`) measured at
    // 25-40ms per call against a local SurrealDB (examples/perf_probe.rs),
    // paid by every authenticated request — so sessions are cached per
    // tenant, contra PLAN.md's original "sessions are cheap, do not cache"
    // assumption. See docs/adr/0006-tenant-session-cache-and-search-split.md.
    tenant_sessions: moka::future::Cache<String, Arc<Surreal<Any>>>,
}

impl Store {
    /// Retries the whole connect → signin → use_ns/use_db → system-db DEFINE
    /// sequence on failure — every step is idempotent, so restarting from the
    /// top after a partial failure (e.g. connected but signin failed) is
    /// safe. Backoff doubles from 500ms to a 5s cap; gives up and returns the
    /// last error once 30s have elapsed since the first attempt (the process
    /// then exits and the orchestrator restarts it, which is correct past
    /// the deadline). Every kind of error is retried, not just ones that
    /// look transient: reliably distinguishing "SurrealDB isn't up yet" from
    /// "wrong password" via SDK error types isn't worth the fragility, and
    /// the warn log on every attempt still surfaces a misconfiguration
    /// immediately even while retrying.
    pub async fn connect(cfg: &DbConfig) -> surrealdb::Result<Self> {
        let start = Instant::now();
        let mut backoff = CONNECT_INITIAL_BACKOFF;
        let mut attempt: u32 = 1;
        loop {
            match Self::try_connect(cfg).await {
                Ok(store) => return Ok(store),
                Err(err) => {
                    if start.elapsed() >= CONNECT_DEADLINE {
                        return Err(err);
                    }
                    tracing::warn!(
                        attempt,
                        error = %err,
                        "surrealdb connect attempt failed, retrying"
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(CONNECT_MAX_BACKOFF);
                    attempt += 1;
                }
            }
        }
    }

    async fn try_connect(cfg: &DbConfig) -> surrealdb::Result<Self> {
        // `surrealdb::engine::any::connect` (unlike `Surreal::new::<Ws>`)
        // parses a full scheme-prefixed URL as-is — the Ws-specific endpoint
        // impl instead always prepends "ws://" itself, so handing it an
        // already-prefixed URL doubles the scheme and fails DNS on the
        // literal host "ws". `any::connect` is the correct entry point for a
        // config-driven URL like ours that already carries its scheme.
        let db = surrealdb::engine::any::connect(cfg.url.as_str()).await?;
        db.signin(Root {
            username: cfg.user.clone(),
            password: cfg.pass.clone(),
        })
        .await?;
        db.use_ns(&cfg.ns).use_db(SYSTEM_DB).await?;
        // SurrealDB 3.x errors "table does not exist" on SELECT against a
        // table that has never been created — unlike CREATE, it does not
        // auto-vivify one. The tenant registry's first-ever read (a lookup
        // for an org id that isn't registered yet) would hit exactly that
        // on a brand-new `system` db, so define it eagerly, idempotently.
        // `.check()` is required — statement errors live inside the
        // Response, not the outer Result, so a bare `.await?` here would
        // silently swallow a failed DEFINE and surface as a confusing
        // "table does not exist" much later at the first real query.
        db.query("DEFINE TABLE IF NOT EXISTS tenant SCHEMALESS")
            .await?
            .check()?;
        // Belt-and-suspenders alongside TenantProvisioner's cache: its
        // single-flight coalescing only guards a single process, so it
        // can't stop two instances (or a restart racing a still-in-flight
        // request) from both provisioning the same org id. This index is
        // the actual guarantee; tenant_repo::create() treats a violation of
        // it as "someone else already won, fetch their row".
        db.query(format!(
            "DEFINE INDEX IF NOT EXISTS {TENANT_ORG_ID_INDEX} ON tenant FIELDS org_id UNIQUE"
        ))
        .await?
        .check()?;
        Ok(Self {
            root: db,
            ns: cfg.ns.clone(),
            tenant_sessions: moka::future::Cache::builder().max_capacity(10_000).build(),
        })
    }

    /// Session for the shared `system` db (tenant registry).
    pub fn system(&self) -> Surreal<Any> {
        self.root.clone()
    }

    /// Uncached, dedicated session for one tenant db — for long-lived
    /// consumers (the WS hub's live queries) that must never share a
    /// session with request traffic. Cloned from `root` (first-generation,
    /// safe per ADR 0002), never from a cached tenant session. The caller
    /// must keep the session alive for as long as its live queries run —
    /// dropping it detaches the server-side session and silently ends
    /// notification delivery (ADR 0008).
    pub async fn dedicated_for_tenant(
        &self,
        tenant_db: &str,
    ) -> surrealdb::Result<Arc<Surreal<Any>>> {
        let session = self.root.clone();
        session.use_ns(&self.ns).use_db(tenant_db).await?;
        Ok(Arc::new(session))
    }

    /// Session for one tenant db, cached across requests. The `Arc` is how
    /// callers share it: cloning the `Arc` is free and safe, while calling
    /// `.clone()` on the `Surreal` inside would create a second-generation
    /// session clone, which hangs all queries (ADR 0002) — never unwrap and
    /// re-clone it. Concurrent use of one session is safe here because the
    /// repos only run self-contained statements (no session variables, no
    /// interactive transactions).
    pub async fn for_tenant(&self, tenant_db: &str) -> surrealdb::Result<Arc<Surreal<Any>>> {
        if let Some(session) = self.tenant_sessions.get(tenant_db).await {
            return Ok(session);
        }
        // Concurrent first requests may both build a session; `insert` lets
        // one win and the loser's session just serves its own request once.
        // Cheaper than single-flight plumbing for a benign race.
        let session = self.root.clone();
        session.use_ns(&self.ns).use_db(tenant_db).await?;
        let session = Arc::new(session);
        self.tenant_sessions
            .insert(tenant_db.to_string(), Arc::clone(&session))
            .await;
        Ok(session)
    }
}
