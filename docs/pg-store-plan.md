# Postgres/Supabase store — switchable alternative backend

Instructions for the implementing agent. Goal: an **alternative store implementation
on PostgreSQL (hosted on Supabase)** that can be switched on and off via config, so
both backends can be compared until we choose one. This is an experiment track — the
SurrealDB path stays the default and must keep working unmodified throughout.

## Ground rules

- Work the steps **in order**; every step ends with `just check` green and the
  SurrealDB path fully functional (`DB_BACKEND` unset behaves exactly as today).
- **No frontend changes.** The API contract, the WS protocol, and the error envelope
  in PLAN.md are frozen — the frontend must not be able to tell which backend served
  a request.
- **Auth is untouched.** Clerk + the JWKS middleware (`crates/api/src/auth.rs`,
  `jwks.rs`) are backend-independent and stay exactly as they are.
- `domain` stays free of database types — no `sqlx` types in `domain`, same layering
  rule that keeps `surrealdb` out of it today.
- Record the experiment as an ADR (`docs/adr/0010-postgres-store-experiment.md` or
  next free number): dual-backend setup, schema-per-tenant, in-process change
  publishing, and the exit criteria (perf numbers + DX verdict → choose one, delete
  the other).
- Deviations from this doc: prefer its intent, record the deviation in the ADR, keep
  going.

## Decisions

| Concern | Choice | Rationale |
|---|---|---|
| Postgres access | `sqlx` (PgPool, runtime queries) — **no ORM** | Mirrors the existing hand-written-query repos behind domain traits; an ORM would duplicate the domain layer |
| Tenancy | **Schema-per-tenant** in one database; shared `system` schema for the tenant registry | Preserves "no tenant predicate on any query" and cheap provisioning; maps 1:1 onto `Store::for_tenant` |
| Tenant scoping | `SET LOCAL search_path` via `set_config($1, true)` **inside a transaction, per request** | Pool-safe: cannot leak across pooled connections. Session-level `SET` is forbidden (grep-able invariant, same class as the `use_ns`/`use_db` rule) |
| Schema name | Same derivation as today: `tenant_` + first 12 hex of SHA-256(org id) | Reuse the existing function; registry rows stay compatible |
| Live updates | **In-process publishing**: mutating handlers publish `LiveChange` into the hub after a successful write; no triggers, no LISTEN/NOTIFY | Single-writer architecture makes DB-level capture unnecessary; documented single-instance limitation (see Step 2) |
| FTS | `tsvector` + GIN (prefix `tsquery` for search-as-you-type), `ts_rank` ranking, `ts_headline` highlights, `pg_trgm` for infix (order numbers, ADR-0009 parity) | Covers the M3 contract incl. `<b>`-only highlights; `simple` + `unaccent` config (no stemming — names/numbers/emails, incl. Ukrainian content) |
| Entity storage | `id` as `text` ULID PK (store-generated, as today); `created_at`/`updated_at` as `timestamptz`, converted to RFC 3339 strings at the row→domain boundary; `line_items`, `address`, money values as `jsonb` | Parity-first: mirror the document shape to keep row→domain mapping mechanical. Normalizing is a post-decision task, not part of the experiment |
| Numbering | `counter` table per tenant schema; `INSERT … ON CONFLICT … DO UPDATE SET value = counter.value + 1 RETURNING value` | Atomic, same semantics as the SurrealDB `UPSERT` counter |
| Switch | `DB_BACKEND` env var: `surreal` (default) \| `postgres`; `DATABASE_URL` required iff `postgres` | One process runs one backend; fail fast on missing/invalid config |
| Local dev/tests | Plain `postgres:17` container (compose + testcontainers). Supabase is only the *hosted* target | Supabase-as-Postgres has no local-only features we use; tests must not depend on a SaaS |

### Supabase connection specifics

- Use the **session-mode / direct connection string** (port 5432), not the
  transaction-mode pooler (port 6543): sqlx relies on prepared statements, which
  break under transaction pooling. One long-lived API process needs no external
  pooler — a modest `PgPool` (e.g. `max_connections` 10) is correct.
- Required extensions: `pg_trgm`, `unaccent` (both available on Supabase; create
  them in migration 0001 with `CREATE EXTENSION IF NOT EXISTS`).
- Nothing else Supabase-specific is used — no PostgREST, no RLS, no Realtime, no
  Supabase Auth. The same `DATABASE_URL` mechanism must work against any Postgres.

---

## Step 0 — Prep refactor (backend-neutral seams; SurrealDB-only, no behavior change)

Three seams, all flagged in `docs/database-portability.md`, landed as their own
commit(s) before any Postgres code exists. Existing tests must pass unmodified.

1. **Move the change types to `domain`.** `ChangeAction`, `ChangeEvent<T>`,
   `LiveChange` currently live in `crates/surreal-store/src/live.rs`. Move the type
   definitions to `crates/domain/src/live.rs` (they already carry only domain
   entities); `surreal-store` re-exports them so its public API is unchanged. The
   hub (`crates/api/src/ws/hub.rs`) switches its imports to `domain`.

2. **Repos as trait objects.** Replace the concrete-typed `repo_for()` helpers in
   `crates/api/src/routes/{customers,orders,invoices,search}.rs` (and the direct
   `SurrealTenantRepo` construction in `api/src/lib.rs`) with a backend facade owned
   by `AppState`:

   ```rust
   #[async_trait]
   pub trait Backend: Send + Sync {
       async fn customer_repo(&self, tenant_db: &str) -> Result<Arc<dyn CustomerRepo>, DomainError>;
       async fn order_repo(&self, tenant_db: &str)    -> Result<Arc<dyn OrderRepo>, DomainError>;
       async fn invoice_repo(&self, tenant_db: &str)  -> Result<Arc<dyn InvoiceRepo>, DomainError>;
       fn tenant_repo(&self) -> Arc<dyn TenantRepo>;
       async fn provision_tenant(&self, org_id: &str, org_name: &str) -> Result<Tenant, DomainError>;
       async fn ping(&self) -> Result<(), DomainError>;
   }
   ```

   Adjust names/signatures to whatever the current `TenantProvisioner` and auth
   middleware actually need — the contract is "auth and routes never name a
   store-crate type again". `SurrealBackend` wraps the existing `Store` +
   `TenantProvisioner` and is the only implementation for now. `AppState.store` /
   `AppState.provisioner` are replaced by `backend: Arc<dyn Backend>`.

3. **`ChangePublisher` seam for the hub.** Handlers get a uniform way to announce
   mutations; which backend is active decides whether it does anything:

   ```rust
   pub trait ChangePublisher: Send + Sync {
       fn publish(&self, tenant_db: &str, change: LiveChange);
   }
   ```

   - `NoopPublisher` — used in surreal mode (live queries already produce events;
     publishing here would double-emit).
   - `HubPublisher` (Step 2) — used in postgres mode.

   Add `publisher: Arc<dyn ChangePublisher>` to `AppState` and call it from every
   mutating handler **after** the service call succeeds, with the returned entity:
   customers create/update/delete; orders create/update/delete + status transition;
   invoice create-from-order, update, status transition. Delete publishes
   `data: None`. With `NoopPublisher` wired, behavior is provably unchanged — the
   existing WS integration tests are the proof.

**Done when:** `just check` and `just test-int` green with zero test edits; grep
confirms no `surreal_store::Surreal*Repo` references outside the `SurrealBackend`
module; the WS acceptance flows still work in dev mode.

## Step 1 — `crates/pg-store`: connection, migrations, provisioning, registry

New workspace crate `pg-store` (add to `backend/Cargo.toml` members), depending on
`domain`, `sqlx` (features: `runtime-tokio`, `tls-rustls`, `postgres`, `json`,
`time` or `chrono` — match what the repo already uses), `ulid`, `sha2`.

- `PgStore::connect(database_url) -> Result<PgStore>` — builds the `PgPool`, runs
  **system-schema migrations** (below), fails fast on error.
- **Migrations**: ordered files `crates/pg-store/migrations/system/*.sql` (registry,
  extensions) and `crates/pg-store/migrations/tenant/*.sql` (per-tenant tables,
  indexes, FTS). Hand-rolled runner, mirroring `surreal-store/src/migrations.rs`:
  each tenant schema carries a `_migrations(version int primary key, applied_at
  timestamptz)` table; the runner applies pending files transactionally. sqlx's
  built-in migrator is not used (it can't loop over schemas).
- **Tenant scoping helper** — the one place `search_path` is ever touched:

  ```rust
  pub async fn tenant_tx(&self, tenant_schema: &str) -> Result<Transaction<'_, Postgres>, DomainError> {
      let mut tx = self.pool.begin().await.map_err(store_err)?;
      sqlx::query("SELECT set_config('search_path', $1, true)")
          .bind(tenant_schema)
          .execute(&mut *tx).await.map_err(store_err)?;
      Ok(tx)
  }
  ```

  `set_config(…, true)` is transaction-local (`SET LOCAL`): it cannot survive the
  transaction, so a pooled connection can never carry another tenant's search_path.
  Repos receive work through this helper only; **no repo query names a schema**.
- **Provisioning** (`provision.rs`): same flow as `surreal-store/src/provision.rs` —
  derive `tenant_<12hex>` from the org id (reuse/move the existing derivation so
  both backends compute identical names), `CREATE SCHEMA IF NOT EXISTS`, run tenant
  migrations, insert the registry row; guarded by the same per-org-id mutex pattern.
- **Registry**: `system.tenant` table matching the domain `Tenant` shape
  (`org_id` unique, `db_name`, `name`, `default_language`, `default_currency`,
  prefixes/settings — mirror whatever `domain::tenant` currently defines);
  `PgTenantRepo` implements `TenantRepo`.
- Tenant tables (tenant migration 0001+), matching the **current** domain structs
  (including anything M5.1 has landed by then — the domain traits are the source of
  truth, not the snapshot in PLAN.md):

  ```sql
  CREATE TABLE customer (
    id          text PRIMARY KEY,
    name        text NOT NULL,
    contact_name text, email text, phone text,
    address     jsonb, notes text,
    created_at  timestamptz NOT NULL, updated_at timestamptz NOT NULL
  );
  -- order is a reserved word: always write "order" quoted, or name the table orders
  -- and keep the mapping inside pg-store (pick one, use it consistently).
  ```

  plus `order`/`invoice`/`exchange_rate`/`counter` per the data model. Seed
  `exchange_rate` at provisioning exactly as the SurrealDB provisioner does.

**Done when:** a `pg-store` integration test (`#[ignore]`, testcontainers
`postgres:17`) provisions two tenants, asserts both schemas exist with the full
table set and independent `_migrations` state, and re-provisioning the same org id
is a no-op returning the existing registry row.

## Step 2 — Entity repos, counters, FTS, in-process publishing

- `PgCustomerRepo` / `PgOrderRepo` / `PgInvoiceRepo` implementing the domain traits.
  Every method opens `tenant_tx`, runs its queries, commits. List endpoints keep the
  contract: pagination, sort allow-list (map `-field` to `DESC`; **never** format
  user input into SQL — sort fields come from a fixed match, everything else is
  bound), `total` via `count(*)` in the same transaction.
- **Counters** (numbering):

  ```sql
  INSERT INTO counter (id, value) VALUES ($1, 1)
  ON CONFLICT (id) DO UPDATE SET value = counter.value + 1
  RETURNING value;
  ```

- **FTS** (tenant migration, one per searchable entity):

  ```sql
  CREATE TEXT SEARCH CONFIGURATION tenant_search (COPY = simple);
  ALTER TEXT SEARCH CONFIGURATION tenant_search
    ALTER MAPPING FOR word, hword, hword_part WITH unaccent, simple;

  ALTER TABLE customer ADD COLUMN search tsvector GENERATED ALWAYS AS (
    to_tsvector('tenant_search',
      coalesce(name,'') || ' ' || coalesce(contact_name,'') || ' ' ||
      coalesce(email,'') || ' ' || coalesce(address->>'city',''))
  ) STORED;
  CREATE INDEX customer_search_idx ON customer USING gin (search);
  CREATE INDEX customer_name_trgm ON customer USING gin (name gin_trgm_ops);
  ```

  Query shape: build a prefix tsquery from the user's input (`websearch_to_tsquery`
  is not prefix-capable — construct `to_tsquery('tenant_search', $terms)` where each
  sanitized term gets `:*` appended), rank with `ts_rank`, highlight with
  `ts_headline('tenant_search', source_text, query)` (default `<b>`/`</b>` markers —
  matches the protocol's "only `<b>` tags"). Order-number infix search uses
  `number ILIKE '%' || $1 || '%'` backed by the trigram index (ADR-0009 parity).
  The `q`-param list queries and the omnibox `search()` trait methods both go
  through this; keep the SQL in one module per entity.
- **`PgBackend`** implements the Step 0 `Backend` trait over `PgStore`.
- **`HubPublisher`**: implements `ChangePublisher` by handing the `LiveChange` to
  the hub. The hub needs an *externally-fed* mode: entries created by `subscribe`
  without spawning a stream-factory task (make `TenantEntry.task` an
  `Option<JoinHandle<()>>`), plus:

  ```rust
  pub async fn publish(&self, tenant_db: &str, change: LiveChange) {
      // No entry = no subscribers = drop the event; nothing to miss.
      if let Some(entry) = self.tenants.lock().await.get(tenant_db) {
          let _ = entry.tx.send(Arc::new(to_server_event(change)));
      }
  }
  ```

  Constructor decides the mode: `Hub::new(store)` (surreal, unchanged) vs
  `Hub::external()`. Existing hub unit tests stay green; add unit tests for the
  external mode (publish with/without subscribers, entry cleanup on last
  unsubscribe).
- Document the known limitation in the ADR: in-process publishing is
  **single-API-instance** delivery. The recorded upgrade path when scaling out is
  `NOTIFY` inside the write transaction + a `LISTEN`er feeding each instance's hub —
  not part of this experiment.

**Done when:** `pg-store` integration tests cover CRUD round-trips for all three
entities, counter atomicity under concurrent creates (spawn N tasks, assert N
distinct sequential numbers), delete guards (customer-with-orders → conflict), and
FTS ranking (prefix match beats mid-word; highlight contains `<b>`). Hub unit tests
cover external mode. `just check` green.

## Step 3 — Wire the switch in `api`

- `config.rs`: add `db_backend: DbBackend` (enum `Surreal | Postgres`, parsed from
  `DB_BACKEND`, default `Surreal`) and `database_url: Option<String>`. Startup rules:
  `postgres` without `DATABASE_URL` → fail fast with a clear message; `surreal`
  ignores `DATABASE_URL`.
- `lib.rs`/startup: build the matching `Backend`, `Hub`, and `ChangePublisher`
  triple — (`SurrealBackend`, `Hub::new(store)`, `NoopPublisher`) or (`PgBackend`,
  `Hub::external()`, `HubPublisher`). Everything downstream is already
  backend-neutral after Step 0.
- `deploy/compose.yaml`: add a `postgres:17` service (healthchecked, volume,
  `POSTGRES_PASSWORD`), so `DB_BACKEND=postgres DATABASE_URL=postgres://…@localhost:5432/polymix`
  works locally. Do not remove or alter the surrealdb service. Optional: a
  `just dev-pg` recipe mirroring `just dev`.
- `.env`/README note for the Supabase target: paste the Supabase **direct/session**
  connection string into `DATABASE_URL`; everything else identical.

**Done when:** with `DB_BACKEND=postgres` against local compose, the M0/M1
acceptance flow passes manually: `POST /dev/token` → `GET /api/me` auto-provisions
the tenant schema; customer CRUD + validation errors work from the UI; two dev orgs
are isolated; and with `DB_BACKEND` unset everything behaves exactly as before.

## Step 4 — Test-suite parity + CI

- Parameterize the `api` integration-test harness (`crates/api/tests/common/mod.rs`)
  by `TEST_DB_BACKEND` (`surreal` default | `postgres`): it starts the matching
  testcontainer and builds the matching `AppState`. The HTTP-level tests (CRUD,
  validation, transitions, numbering, isolation, search, WS create/update/delete +
  tenant isolation) must pass **unchanged under both values** — they are the parity
  proof for the whole experiment.
- Backend-specific tests get an explicit gate (skip-with-log when the other backend
  is selected): the SurrealDB pause/resync resilience test stays surreal-only
  (postgres mode has no DB stream to lose); add a postgres-only WS test asserting a
  REST mutation reaches a connected WS client via the in-process path.
- CI: extend the `test-int` job to a two-value matrix over `TEST_DB_BACKEND`
  (same timeout/concurrency rules as today). Both must be green for `build` to run.

**Done when:** `just test-int` passes locally with both `TEST_DB_BACKEND` values;
CI runs both jobs green.

## Step 5 — Seeder, perf comparison, verdict inputs

- Extend `crates/seeder` to honor the same `DB_BACKEND`/`DATABASE_URL` config and
  seed the demo tenant (50k customers / 200k orders, batched 1000) into Postgres.
- Run `scripts/perf-search.sh` against the postgres backend (the `perf-check` skill
  automates the surreal side; replicate its flow with the pg env). Record results in
  `docs/perf.md` as a new "Postgres backend" section using the same table format —
  omnibox + per-entity p50/p95/p99, debug and release — next to the SurrealDB
  numbers, with a pass/fail against the 100 ms NFR.
- Finish the ADR with the comparison data and the open decision: which backend is
  chosen, and that the loser's crate + config path is to be deleted (both-forever is
  not an outcome).

**Done when:** `docs/perf.md` shows both backends side by side; the ADR is complete
except the final verdict; `just check` + both `just test-int` matrices green.

## Explicitly out of scope for this experiment

- LISTEN/NOTIFY, logical replication, Supabase Realtime — in-process publishing only.
- RLS, PostgREST, Supabase Auth, Edge Functions — Supabase is hosted Postgres here.
- Normalizing `line_items`/`address` into relational tables.
- Multi-instance API deployment, M6 hardening changes for the pg path (readiness/
  startup-retry adjustments happen after the verdict, on whichever backend wins).
- Any frontend change, any WS-protocol change, any auth change.
