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

## 9. `FULLTEXT` index quirks: one field per index, unique match refs, and a `count()` planner bug

PLAN.md's M3 spec writes `DEFINE INDEX ... SEARCH ANALYZER ... BM25
HIGHLIGHTS` with a multi-field `FIELDS` list, and a query shape that reuses
one match reference (`@0@`) across every predicate. None of that survives
contact with `surrealdb/surrealdb:v3.2` unchanged — see
`docs/adr/0001-surrealdb-fulltext-keyword.md` for the full writeup and the
decision. Four points, all confirmed against a live container and cross-
checked against `surrealdb-core-3.2.0/src/syn/parser/stmt/define.rs`:

1. The keyword is `FULLTEXT`, not `SEARCH` (`SEARCH` isn't recognized at
   all at that grammar position).
2. A `FULLTEXT` index takes exactly **one** field — `DEFINE INDEX ... ON t
   FIELDS a, b FULLTEXT ...` fails with `Expected one column, found 2`. One
   index per searchable field, not one index per table.
3. Match references (`@N@`) must be **unique per query** across different
   indexes — reusing `@0@` for predicates on two different fields/indexes
   fails with `Duplicated Match reference: 0`. Give each field its own `N`
   and sum `search::score(N)` for a combined rank.
4. `ORDER BY search::score(0) DESC` alone fails
   (`Missing order idiom \`search\` in statement selection`) — the score
   expression must be projected first: `SELECT *, (search::score(0) + ...)
   AS score ... ORDER BY score`.

A fifth issue is a genuine planner bug, not a spec mismatch:
**`SELECT count() FROM t WHERE <fulltext predicate> GROUP ALL` returns
`{"count": 0}` even when rows match.** Confirmed by running the identical
`WHERE` clause as `SELECT *` (returns the matching rows) right next to the
`count()` form (returns `0`) in the same request. Workaround: wrap the
predicate in a subquery —
`SELECT count() FROM (SELECT id FROM t WHERE <predicate>) GROUP ALL`
returns the correct count. Only affects count-only aggregates over a
full-text predicate; a plain `SELECT count() ... GROUP ALL` with no `@N@`
operator in its `WHERE` is unaffected.

## 10. The `ascii` analyzer filter does not damage non-Latin scripts

Worth confirming before assuming `FILTERS lowercase, ascii, edgengram(...)`
is Latin/English-only: it isn't. Tested interactively against
`surrealdb/surrealdb:v3.2` with Cyrillic content (`"Адамант Print GmbH"`)
indexed through an analyzer that includes `ascii`:

- Cyrillic queries match and case-fold correctly: `"адам"` and `"АДАМ"`
  both matched via edge-ngram prefix search, same as they would through an
  analyzer with the `ascii` filter removed.
- The `ascii` filter's actual effect is Latin-diacritic folding only —
  `"café"` matched a query for `"cafe"` through the `ascii`-filtered field,
  but the same query did *not* match an otherwise-identical field indexed
  without `ascii`. Non-Latin code points pass through unchanged; they are
  not stripped, mangled, or excluded from the index.

Practical implication: PLAN.md's `autocomplete` analyzer (`class`
tokenizer, `lowercase, ascii, edgengram(2, 10)`) needs no adjustment to
support the `ua` locale — the `ascii` filter is pure upside for mixed
Latin/Cyrillic tenant data (it still folds diacritics in Latin-script
customer/company names) with no downside for Cyrillic search.

## 11. How these were confirmed

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

## 12. Cloning an already-cloned session hangs queries — clone from root only, once

Building on §1's pattern: `clone()` forks a new server-side session by
sending a `SessionId::Clone { old, new }` event that the connection's router
task replays (`Attach` + recorded `use_ns`/`use_db`/etc. commands) onto a
fresh session id (`surrealdb-3.2.0/src/engine/remote/ws/mod.rs`,
`handle_session_clone`). That works correctly exactly once — cloning
`root` for a per-request/tenant session, as §1 prescribes.

**Cloning that already-cloned session again — a second-generation
clone/grandchild of `root` — causes every subsequent query issued on it to
hang indefinitely**, even though the query text, bindings, and underlying
data are identical to ones that succeed on the first-generation session.
Confirmed empirically with a standalone example reusing the real crates on
the same live connection:

- Query run directly on the session returned by `for_tenant()` (one clone
  from `root`): consistently fast (~100-200ms).
- The exact same query, run on `session.clone()` of that session (two clones
  from `root`) — via a raw inline query, *not* going through any repo/trait
  code: hangs (no response, ever).
- A **fresh** `store.for_tenant(...)` call (a new first-generation clone of
  `root`) instead of `.clone()`-ing the existing session: fast again.

This was not a query-shape issue (ruled out `search::highlight()`,
`SurrealValue` deserialization, and `type::table($table)` individually —
all fast in isolation) and not `async_trait` dispatch (a hand-inlined copy
of the trait method's body hung identically when run on a double-clone).
Root-caused to the clone depth itself. Not yet filed upstream; treat as an
open SurrealDB 3.2 SDK/server bug rather than something to route around
case-by-case — **never call `.clone()` on a session that is itself already
a clone; call `for_tenant()` (or equivalent root-clone) again instead.**
See `docs/adr/0002-surrealdb-session-clone-depth.md`.
