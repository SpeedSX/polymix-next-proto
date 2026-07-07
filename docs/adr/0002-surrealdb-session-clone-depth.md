# 0002 — `/api/search` hung: never clone a session that is itself already a clone

## Status

Accepted.

## Context

The omnibox endpoint (`GET /api/search`, `crates/api/src/routes/search.rs`)
hung indefinitely for every non-empty query, with no error, no timeout, and
no server-side CPU/log activity — the request simply never returned, while
every other endpoint (customer/order/invoice list, get, create) worked
normally against the same data and the same SurrealDB connection.

Isolated with a standalone example reusing the real `surreal-store` crate on
a single live connection, progressively reproducing more of the real
`SurrealCustomerRepo::search()` implementation:

- The raw query (with `search::highlight()`, typed `SurrealValue`
  deserialization, and `FROM type::table($table)` — every structural piece
  of the real query) ran fast (~100-200ms) when issued directly on the
  session returned by `Store::for_tenant()`.
- The identical query hung indefinitely as soon as it was issued on
  `session.clone()` of that same session — reproduced with a hand-inlined
  copy of the query, bypassing the repo trait and `async_trait` entirely, so
  neither of those was the cause.
- A **fresh** `store.for_tenant(...)` call in place of the `.clone()`
  succeeded fast again.

Root cause: `search.rs` was the only handler in the codebase that called
`.clone()` on a session already returned by `for_tenant()` — once per extra
repo (`SurrealCustomerRepo::new(session.clone())`,
`SurrealOrderRepo::new(session.clone())`). Every other handler calls
`for_tenant()` once and uses that session directly, per the pattern in
`docs/surrealdb-rust-sdk-notes.md` §1 (clone `root` once per
request/tenant). `for_tenant()` itself is implemented as `root.clone()` +
`use_ns`/`use_db`, so the sessions handed to the customer/order repos in
`search.rs` were **second-generation clones** (clones of a clone of `root`)
— and cloning an already-cloned session, per §12 of the same notes doc, is
what triggers the hang. The invoice repo (last in the sequence, given the
original un-cloned session by move) would have worked fine, but the
sequential `.await`s never reached it because the first call already hung
forever.

## Decision

Stop cloning the session returned by `for_tenant()` in `search.rs`. Call
`for_tenant()` fresh for each of the three repos instead — one
first-generation clone of `root` per repo, matching every other handler in
the codebase:

```rust
let customer_session = state.store.for_tenant(tenant_db).await.map_err(...)?;
let customers = SurrealCustomerRepo::new(customer_session).search(q, HITS_PER_ENTITY).await?;

let order_session = state.store.for_tenant(tenant_db).await.map_err(...)?;
let orders = SurrealOrderRepo::new(order_session).search(q, HITS_PER_ENTITY).await?;

let invoice_session = state.store.for_tenant(tenant_db).await.map_err(...)?;
let invoices = SurrealInvoiceRepo::new(invoice_session).search(q, HITS_PER_ENTITY).await?;
```

This is a workaround for what looks like a genuine SurrealDB 3.2 SDK/server
bug (session-clone depth breaking query/response routing), not a deliberate
architectural choice — see `docs/surrealdb-rust-sdk-notes.md` §12 for the
full isolation trail. Not filed upstream yet.

## Consequences

- `/api/search` issues three `use_ns`/`use_db` round-trips (one per
  first-generation `for_tenant()` call) instead of one round-trip plus two
  cheap clones. Each added round-trip is small relative to the query cost
  itself; acceptable for a 3-entity omnibox fan-out.
- The rule going forward, everywhere in this codebase: **a session handed to
  a repo must be either `root`'s direct clone (`for_tenant()`'s return
  value) or that value itself — never `.clone()` it again.** If a handler
  needs the same tenant session in more than one repo, call `for_tenant()`
  again rather than cloning the one already in hand.
- If the SurrealDB Rust SDK is upgraded, re-run the isolation from
  `docs/surrealdb-rust-sdk-notes.md` §12 before removing this workaround —
  it may be fixed upstream, but don't assume so without checking.
