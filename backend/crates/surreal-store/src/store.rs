use surrealdb::Surreal;
use surrealdb::engine::any::Any;
use surrealdb::opt::auth::Root;

pub struct DbConfig {
    pub url: String,
    pub user: String,
    pub pass: String,
    pub ns: String,
}

const SYSTEM_DB: &str = "system";

/// Root-authenticated session pinned to the `system` db. Never handed out
/// directly — tenant sessions are cloned from it (SDK >= 3.0: a clone gets
/// its own independent namespace/database selection on the same underlying
/// connection, so concurrent tenant sessions never affect each other).
pub struct Store {
    root: Surreal<Any>,
    ns: String,
}

impl Store {
    pub async fn connect(cfg: &DbConfig) -> surrealdb::Result<Self> {
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
        db.query("DEFINE TABLE IF NOT EXISTS tenant SCHEMALESS")
            .await?;
        Ok(Self {
            root: db,
            ns: cfg.ns.clone(),
        })
    }

    /// Session for the shared `system` db (tenant registry).
    pub fn system(&self) -> Surreal<Any> {
        self.root.clone()
    }

    /// Independent session for one tenant db, per request. Sessions are
    /// cheap — do not cache them per tenant.
    pub async fn for_tenant(&self, tenant_db: &str) -> surrealdb::Result<Surreal<Any>> {
        let session = self.root.clone();
        session.use_ns(&self.ns).use_db(tenant_db).await?;
        Ok(session)
    }
}
