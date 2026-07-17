# Database portability: how isolated is SurrealDB?

Assessment (2026-07-12) of what it would take to move the backend off
SurrealDB to PostgreSQL if SurrealDB doesn't pay off.

**Verdict: the isolation is real.** A PostgreSQL swap would be a new store
crate plus a handful of mechanical touch-ups in `api`, not a rewrite. The
genuinely hard part isn't code layering but re-implementing three SurrealDB
*capabilities* (live queries, BM25 search, DB-per-tenant) that Postgres does
differently.

## What's cleanly isolated

- **Dependency boundary.** The `surrealdb` crate is a dependency of only
  `surreal-store` and `seeder`. `domain` and `api` never depend on it
  directly (`backend/Cargo.toml`).
- **Contracts live in `domain`.** `CustomerRepo`, `OrderRepo`,
  `InvoiceRepo`, `TenantRepo` traits use only domain types — `String` ids,
  `DomainError`, `Paged<T>`, `SearchHit` (`domain/src/customer.rs`,
  `order.rs`, `invoice.rs`, `tenant.rs`). No `RecordId`, no `Datetime`,
  nothing Surreal-shaped.
- **Route handlers program against the traits.** Handlers call
  `repo.list()`, `repo.create()` etc. via the domain traits; DB errors are
  flattened into `DomainError::Store(String)` inside the store crate, so
  `api/src/error.rs` knows nothing about SurrealDB errors.
- **Live updates are domain-typed.** `domain/src/live.rs` owns `LiveChange` /
  `ChangeEvent<Customer|Order|Invoice>`, which carry only domain entities;
  `surreal-store/src/live.rs` only maps database notifications into those
  types. The WS hub never sees `RecordId` or the row structs.
- **All SurrealQL is confined** to `surreal-store` (repos, migrations,
  provisioning). No query strings anywhere in `api`.

## Remaining SurrealDB-specific API wiring

1. **The WS stream producer.** `api/src/ws/hub.rs` still constructs
   SurrealDB live-query streams from `Store`; the planned external hub mode
   removes that dependency for PostgreSQL while preserving it for SurrealDB.
2. **Startup construction.** `api/src/lib.rs` builds the SurrealDB store and
   hub before erasing repository access behind `Arc<dyn Backend>`. Routes,
   auth, and `AppState` no longer expose concrete store types.
3. **Config naming.** `SURREALDB_URL/USER/PASS/NS` env vars in
   `api/src/config.rs` — cosmetic.
4. **`seeder`** talks to SurrealDB directly and would be rewritten (it's a
   dev tool, acceptable).
5. **Integration tests** spin up SurrealDB via testcontainers
   (`api/tests/common/mod.rs`) — the harness would swap to a Postgres
   container; the tests themselves exercise the HTTP API and mostly carry
   over.

## The real migration cost: capability gaps, not coupling

- **Live queries.** The SurrealDB hub is built on `LIVE SELECT`. The
  PostgreSQL experiment instead publishes successful mutations from API
  handlers into an external-mode in-process hub. This deliberately supports
  one API instance; scaling out would require transactional `NOTIFY` plus a
  `LISTEN` consumer per instance.
- **Search.** The `search()` trait methods promise "BM25-ranked hits".
  Postgres would use `tsvector` ranking or `pg_trgm` — the interface
  holds, but ranking semantics (and the perf numbers in `docs/perf.md`)
  would shift.
- **Tenancy model.** Database-per-tenant maps to schema-per-tenant in
  Postgres; `Store::for_tenant`'s session cache becomes
  `search_path`-scoped connections, and `TenantProvisioner` / `migrations`
  get reimplemented per-schema. Straightforward, but real design work —
  and several ADRs (0002, 0006, 0008) encode Surreal-specific session
  semantics that simply stop applying.

## Migration blast radius, summarized

The prep refactor is complete: `AppState` owns a backend facade and a
change-publisher seam, and routes use trait-object repositories. Adding the
PostgreSQL store is now additive apart from config/startup selection, the
external hub mode, seeder support, and test-harness parameterization.
