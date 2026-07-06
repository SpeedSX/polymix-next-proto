# SurrealDB Rust SDK (3.x) — implementation notes

Working notes gathered while building `crates/surreal-store` against `surrealdb = "3"`
(resolved to 3.2.0, July 2026). Kept separate from PLAN.md because these are SDK
mechanics, not architecture decisions — useful raw material for a skill or
knowledge-base entry on "using the SurrealDB Rust SDK 3.x".

## 1. Multi-tenant sessions: the version boundary that matters

- **SDK 2.x:** `Surreal<C>::clone()` clones a handle to the *same* session.
  `use_ns()`/`use_db()` on any clone mutates state shared by every other
  clone — a shared client with per-request `use_db()` races across tenants.
- **SDK 3.0+:** `clone()` creates a **new, independent session** (namespace,
  database, auth, session vars, transactions) on the same underlying
  connection. The clone inherits current session state at the moment of
  cloning, then diverges. This is the documented pattern for per-tenant
  isolation on one shared connection:
  <https://surrealdb.com/docs/languages/rust/concepts/multi-tenancy>

  Practical pattern (what `surreal-store::Store` does): keep one root session
  pinned to a shared `system` db; for each request/tenant, `let session =
  root.clone(); session.use_ns(ns).use_db(tenant_db).await?;` and hand that
  session to the repos for that request. Sessions are cheap — don't cache
  them per tenant, just clone on demand.

- If a downgrade to 2.x is ever forced, this whole pattern breaks and must be
  replaced with one cached *connection* per tenant (`RwLock<HashMap<String,
  Surreal<Client>>>`) — record that as its own ADR, don't half-migrate.

## 2. SurrealDB 3.x: `SELECT` on a never-created table errors, it does not return empty

This is server behavior (confirmed against `surrealdb/surrealdb:v3.2`), not
Rust-SDK-specific — it'll bite any client. In a fresh namespace/database, a
table that has never been created (no prior `CREATE`/`INSERT`, no explicit
`DEFINE TABLE`) does not silently behave as "exists but empty" for reads.
Both a wildcard `SELECT * FROM sometable` and a record-id-shaped
`SELECT * FROM sometable:someid` (equivalently, the SDK's
`session.select(("sometable", "someid"))`) return an error:

```
"The table 'sometable' does not exist"
```

Confirmed interactively via `surreal sql`:

```
> SELECT * FROM tenant;
"The table 'tenant' does not exist"
> DEFINE TABLE tenant SCHEMALESS;
> SELECT * FROM tenant;
[]
```

This matters a lot for exactly the kind of code a tenant-registry or
migration-tracking lookup writes: "check if a record exists, and if not,
create it" — the very first check, on a brand-new database, throws instead
of returning `None`/`[]`. `CREATE`/`INSERT`/`UPSERT` auto-vivify the table
fine; only the read path needs the table to already exist.

Fix: eagerly run `DEFINE TABLE IF NOT EXISTS <name> SCHEMALESS` (idempotent
— safe to run on every connect/provision, confirmed no error and no data
loss on repeat calls) before any `SELECT` that might be the first-ever touch
of that table. Two call sites needed this in `surreal-store`: the tenant
registry's `tenant` table in the shared `system` db (defined once in
`Store::connect`) and the per-tenant `meta` table used for migration-version
tracking (defined at the top of `apply_migrations`, since it runs against a
just-created, completely untouched tenant db).

## 3. A unique-index violation is not a structured error kind — match the message

`surrealdb::Error`'s wire format (see `surrealdb_types::error::Error`) is
supposed to be structured: a `kind` (`ErrorDetails` enum — `Validation`,
`NotFound`, `AlreadyExists`, etc.) plus a human `message`. `AlreadyExists`
even has a matching detail enum (`Session | Table | Record | Namespace |
Database`). None of that fires for a `DEFINE INDEX ... UNIQUE` violation,
though — confirmed empirically against `surrealdb/surrealdb:v3.2` by
triggering one through the real client and printing `{:?}`:

```
Error { code: -32000, message: "Database index `tenant_org_id` already
contains 'dup_probe', with record `tenant:probe1`", details: Internal,
cause: None }
```

`details: Internal` — i.e. the generic catch-all, not `AlreadyExists`. The
core crate does have a dedicated `Error::IndexExists { record, index, value
}` variant server-side (`surrealdb-core/src/err/mod.rs`), it just isn't
mapped to the newer typed wire schema yet. So the only signal a client gets
today is the message text. Detect it with something reasonably specific —
matching both the index name and the phrase, not just `"already contains"`
alone — and treat it as "someone else's row won, go fetch it":

```rust
fn is_org_id_conflict(err: &surrealdb::Error) -> bool {
    let message = err.to_string();
    message.contains(TENANT_ORG_ID_INDEX) && message.contains("already contains")
}
```

This matters wherever a unique index is the actual concurrency guarantee
(e.g. a per-org-id in-process mutex only protects one process/instance — the
index is what stops two instances, or a restart racing an in-flight
request, from both creating the same logical row). Revisit this matcher if
a future SDK version does map `IndexExists` to `AlreadyExists` — at that
point `err.already_exists_details()` would be the correct, non-stringly
check.

## 4. `SurrealValue`, not `serde::{Serialize, Deserialize}`, is the DB-facing trait

This was the main compile-time surprise vs. older SurrealDB SDK examples
still floating around online (most are written against 1.x/2.x).

- Every type used with `.select()`, `.create().content()`, `.upsert().content()`,
  or `IndexedResults::take()` must implement `surrealdb::types::SurrealValue`
  — plain `#[derive(Serialize, Deserialize)]` is **not** sufficient and fails
  with `the trait bound ...: SurrealValue is not satisfied`.
- Derive it: `#[derive(surrealdb::types::SurrealValue)]` (re-exported from the
  `surrealdb_types` crate via `surrealdb::types`).
- **Crate path gotcha:** the derive macro defaults to emitting
  `::surrealdb_types::...` paths unless the `sdk-path` feature is active. If
  your crate only depends on `surrealdb` (not `surrealdb_types` directly —
  the normal case for application code), that default path won't resolve.
  Fix: annotate every derive with `#[surreal(crate = "surrealdb::types")]`.

  ```rust
  #[derive(Debug, surrealdb::types::SurrealValue)]
  #[surreal(crate = "surrealdb::types")]
  struct TenantRow {
      id: surrealdb::types::RecordId,
      org_id: String,
      // ...
  }
  ```

- Other `#[surreal(...)]` field/container attributes exist and mirror serde's:
  `rename`, `rename_all`, `skip`, `untagged`, `tag`/`content` for enums.
- Escape hatch: if a type only implements `Serialize + DeserializeOwned` and
  can't be annotated (e.g. it's from another crate), wrap it in
  `surrealdb::types::SerdeWrapper<T>` — there's a blanket `SurrealValue` impl
  for that wrapper, and `#[surreal(wrap)]` does this per-field automatically.
- Primitives (`String`, `i64`, `bool`, `Option<T>`, `Vec<T>`, tuples, etc.)
  and `RecordId`/`RecordIdKey` already implement `SurrealValue` natively —
  only your own structs need the derive.

## 5. `RecordId` / `RecordIdKey`

- Location: `surrealdb::types::RecordId` (**not** `surrealdb::RecordId` — the
  crate root does not re-export it).
- Shape: `struct RecordId { table: Table, key: RecordIdKey }`.
- `RecordIdKey` is an enum: `Number(i64) | String(String) | Uuid(Uuid) |
  Array(Array) | Object(Object) | Range(Box<RecordIdKeyRange>)`. It has
  **no `Display` impl** — don't call `.to_string()` expecting `"table:key"`
  formatting from the key alone. To recover a plain string id (e.g. to expose
  as the API's opaque `id` field, per PLAN.md's id convention), match the
  `String` variant explicitly:

  ```rust
  fn record_key(id: &RecordId) -> String {
      match &id.key {
          RecordIdKey::String(key) => key.clone(),
          other => format!("{other:?}"), // fallback; shouldn't happen for our ULID keys
      }
  }
  ```
- Helpers: `RecordIdKey::ulid()` / `::uuid()` / `::rand()` generate a key
  directly if you don't need to keep the raw ID yourself before the insert.
- `RecordId::new(table, key)` — `table`/`key` accept anything with a matching
  `Into`, including plain `&str`/`String`.

## 6. `Surreal::new::<Ws>(url)` silently double-prefixes a scheme — use `engine::any::connect` for config-driven URLs

If your connection URL comes from config as a full scheme-prefixed string
(e.g. `ws://localhost:8000`, matching how most docs/examples show it), do
**not** pass it to `Surreal::new::<Ws>(url)`. The `Ws`-typed engine's
`IntoEndpoint` impl for `&str`/`String` unconditionally does
`format!("ws://{self}")` (see `opt/endpoint/ws.rs`) — it expects a **bare**
`host:port`, matching the SDK's own doc example
(`Surreal::new::<Ws>("127.0.0.1:8000")`, no scheme). Hand it an
already-prefixed URL and you get `ws://ws://localhost:8000`, which `Url`
parses with host `"ws"` — surfacing as a confusing DNS failure at connect
time (`IO error: No such host is known`) that has nothing to do with the
real host being unreachable.

Fix: use the runtime-dispatched **`Any` engine** instead, which parses a
full scheme-prefixed string correctly (checks `starts_with("ws")` /
`"http"` / `"tikv"` / `"memory"` etc. before deciding how to parse — see
`engine/any/mod.rs`):

```rust
use surrealdb::engine::any::Any;

let db: surrealdb::Surreal<Any> = surrealdb::engine::any::connect(url).await?;
```

This is also simply the better fit when the scheme is config-driven rather
than known at compile time — `Any` is designed for exactly that ("choice of
engine is made at runtime" per the module's own docs), and only needs the
matching protocol feature enabled (`protocol-ws` is on by default).

## 7. `opt::auth::Root` takes owned `String`s

`surrealdb::opt::auth::Root { username: String, password: String }` — not
`&str`. Older sample code (and muscle memory from other SDKs) reaches for
borrowed strings here; clone before constructing it.

## 8. Misc version-pinning gotchas hit while resolving `Cargo.toml`

- `tracing = "1"` does not exist as a version requirement — the crate is
  still on the `0.x` line (`tracing = "0.1"`). Easy typo since `tokio`,
  `serde`, etc. really are at major version 1.
- It's normal and harmless for `jsonwebtoken` to appear **twice** in
  `Cargo.lock` at different versions (e.g. our direct dependency at `9.x`
  alongside `surrealdb`'s own transitive dependency at `10.x`). Cargo allows
  multiple major versions of the same crate to coexist in one dependency
  graph; this is not a conflict to "fix".

## 9. How these were confirmed

Web docs for a fast-moving SDK (three major versions in recent history) were
sometimes stale or summarized ambiguously by the fetch tool. The reliable
tie-breaker was reading the actual installed crate source straight from the
Cargo registry cache, e.g.:

```
~/.cargo/registry/src/index.crates.io-*/surrealdb-3.2.0/src/
~/.cargo/registry/src/index.crates.io-*/surrealdb-types-3.2.0/src/
~/.cargo/registry/src/index.crates.io-*/surrealdb-types-derive-3.2.0/src/
```

When the SDK's own behavior is in question (not just "what's the current
API"), prefer grepping that source over trusting a summarized doc fetch —
then confirm with `cargo check`/`cargo clippy -D warnings` against the real
compiler.
