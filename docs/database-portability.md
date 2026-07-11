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
- **Live updates are domain-typed.** `surreal-store/src/live.rs` enforces
  the layering — the WS hub consumes `LiveChange` /
  `ChangeEvent<Customer|Order|Invoice>`, which carry only domain entities;
  it never sees `RecordId` or the row structs.
- **All SurrealQL is confined** to `surreal-store` (repos, migrations,
  provisioning). No query strings anywhere in `api`.

## Where SurrealDB leaks into `api` (all mechanical to fix)

1. **Concrete repo types instead of trait objects.** Each route file's
   `repo_for()` returns `SurrealCustomerRepo` etc.
   (`api/src/routes/customers.rs`, `orders.rs`, `invoices.rs`,
   `search.rs`), and `api/src/lib.rs` constructs `SurrealTenantRepo`
   directly. That's ~5 small functions to repoint — or generalize to
   `Arc<dyn CustomerRepo>` now to make the swap config-selectable.
2. **`Store` hands out `Surreal<Any>` sessions.** `store.system()` /
   `for_tenant()` / `dedicated_for_tenant()` return `Arc<Surreal<Any>>`,
   which `api` passes opaquely into repo constructors and
   `live_changes()`. `api` never *calls* anything on the session, but it
   names the type transitively. A Postgres store would replace this with a
   pool handle; the call shape stays identical.
3. **Config naming.** `SURREALDB_URL/USER/PASS/NS` env vars in
   `api/src/config.rs` — cosmetic.
4. **`seeder`** talks to SurrealDB directly and would be rewritten (it's a
   dev tool, acceptable).
5. **Integration tests** spin up SurrealDB via testcontainers
   (`api/tests/common/mod.rs`) — the harness would swap to a Postgres
   container; the tests themselves exercise the HTTP API and mostly carry
   over.

## The real migration cost: capability gaps, not coupling

- **Live queries.** The WS hub is built on `LIVE SELECT`; Postgres has no
  equivalent, so a `pg-store` `live_changes()` would need triggers +
  `LISTEN/NOTIFY` (or logical replication / polling). The stream contract
  (`Stream<Item = Result<LiveChange, DomainError>>`) is already
  database-neutral, so only the producer changes — but it's the biggest
  piece of new work.
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

Swap the store crate behind the existing domain traits, touch ~5
`repo_for`-style wiring points, config, seeder, and the test harness.
`domain` and all route/business logic are untouched.

**Optional prep step** if the escape hatch should be even cheaper: switch
`repo_for()` / `AppState` to trait objects (`Arc<dyn CustomerRepo>` and a
small session-factory abstraction over `Store`) — then a Postgres store
becomes purely additive.
