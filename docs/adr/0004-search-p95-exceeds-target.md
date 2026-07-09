# 0004 — `/api/search` p95 exceeds PLAN.md's 100ms target

## Status

Accepted.

## Context

PLAN.md's M3 "Done when" requires `p95 < 100ms for the search endpoint on
the seeded volume (measure with a quick script, record in /docs/perf.md)`.
After fixing the session-clone hang (ADR 0002) and the order/line-items
latency (ADR 0003), `scripts/perf-search.sh` against the 50k-customer/
200k-order seeded tenant measures:

- p50 ≈ 163-167ms, p95 ≈ 254-291ms, p99 ≈ 305-420ms (multiple runs, debug
  and release builds — see `docs/perf.md`).

Three lines of evidence isolate the remaining cost to session-open overhead
and customer search specifically, not query execution or
interpreted-vs-compiled Rust:

- A release build (`cargo build --release`) measured no faster than debug
  (p95 254-291ms release vs. 209-256ms debug across runs) — ruling out
  compiler-optimization headroom as the fix.
- The same search query issued directly against SurrealDB's `/sql` HTTP
  endpoint, bypassing the API and Rust driver entirely, resolves in
  30-55ms.
- `scripts/perf-search.sh` also measures the per-entity search-as-you-type
  endpoints (`GET /api/customers?q=`, `GET /api/orders?q=`) in isolation,
  each behind a single `for_tenant()` call — no fan-out, no clone-depth
  workaround. `/api/orders?q=` meets the target on its own (p95 56ms).
  `/api/customers?q=` does not (p95 228ms), and its p95 is close to the
  omnibox's own p95 (304ms) — customer search, not the fan-out overhead
  alone, is the dominant cost. See `docs/perf.md` for the full breakdown.

Two contributing causes, not one:

- The gap between omnibox and per-entity endpoints is explained by ADR
  0002's workaround: to avoid the session-clone-depth hang, `search.rs`
  calls `Store::for_tenant()` fresh for each of the three repos
  (customer/order/invoice) instead of sharing one session. Each
  `for_tenant()` call is a `root.clone()` plus `use_ns`/`use_db` — two
  extra round-trips per repo, three repos per request, on top of the
  query itself.
- Customer search's own latency (p95 228ms even standalone, single
  session) is not yet explained by session overhead alone. Customer
  search matches four FULLTEXT fields (`name`, `contact_name`, `email`,
  `address.city`) fanned into one `OR`'d condition/score expression,
  against order's two (`number`, `notes`, post-ADR-0003) — not yet
  isolated field-by-field to confirm this is the cause.

## Decision

Accept the current p95 (omnibox ~200-300ms, customer search alone ~230ms)
as a known deviation from PLAN.md's <100ms target rather than block M3 on
it. Record the measured numbers in `docs/perf.md` as actual results, not
the target. Order search alone already meets the target — no action
needed there.

Tested and ruled out:

- **Swapping the SDK's transport to the HTTP engine.** `Store` is built on
  `Surreal<Any>`, so pointing `SURREALDB_URL` at `http://localhost:8000`
  switches the whole request path from WS to HTTP (with the
  `protocol-http` cargo feature enabled — not on by default, so not
  quite a zero-code-change swap). Hypothesis was that the 30-55ms bare
  `/sql` number would carry over if the SDK itself spoke HTTP instead of
  WS. It didn't: every endpoint got *slower*, not faster (omnibox p95
  ~380ms vs. ~256-291ms on ws://, two runs — see `docs/perf.md`). The bare
  `/sql` timing has no session state attached to it; `for_tenant()`'s
  `use_ns`/`use_db` calls still have to run per request, and over HTTP
  that cost isn't amortized across a persistent connection the way it is
  over ws://. The SDK/router overhead this ADR originally flagged as "the
  single biggest unknown" is not fixed by changing transport — the
  session-open cost is inherent to how `for_tenant()` re-establishes
  namespace/db selection every call, independent of WS vs. HTTP.

- **Combining the three-repo fan-out into one session + one multi-statement
  query.** A narrower alternative to the repo-signature refactor rejected
  below: leave `CustomerRepo`/`OrderRepo`/`InvoiceRepo` untouched, and add
  a `Store::search_all()` that bypasses them entirely — one `for_tenant()`
  session, one `.query()` call with all three entities' `SELECT`s joined by
  `;`, read back via `.take(0)`/`.take(1)`/`.take(2)`. Confirmed against the
  `surrealdb-core-3.2.0` source that match references (`@0@`, `@1@`, ...)
  are scoped per-statement by the query planner (`InnerQueryExecutor` is
  built per single `table: TableName`), not per multi-statement request, so
  each entity's existing `@0@`-based WHERE/score expressions could be
  reused verbatim, unrenumbered. Implemented; all of
  `crates/api/tests/search.rs` passed unchanged.

  Measured effect (`scripts/perf-search.sh`, 3 runs, same seeded tenant):
  the omnibox-vs-solo-customer-search ratio flipped, confirming the
  fan-out overhead really was cut. Before: omnibox p95 (216-304ms) was
  consistently *slower* than customer search alone (228ms). After: omnibox
  p95 (263-300ms) was consistently *faster* than customer search alone
  (335-368ms), despite querying three tables instead of one — only
  possible with one session-open instead of three. But the absolute
  omnibox p95 barely moved and stayed 2.5-3x over the 100ms target:
  customer search's own per-query cost (the second, still-unexplained
  cause below) dominates enough that the fan-out savings don't show up
  end-to-end. (The absolute numbers above aren't directly comparable
  across runs — measured on a different machine than the baseline in
  `docs/perf.md` — but the omnibox-vs-customer-alone ratio, taken within
  each run, is.)

  **Reverted.** The fix adds real complexity — a store-level method that
  reaches past the repo trait boundary and reuses each repo's WHERE/score
  constants via new `pub(crate)` exports — for an improvement that doesn't
  move the number PLAN.md's M3 actually gates on (omnibox p95 vs. 100ms).
  Not worth keeping until customer search's own cost is addressed too, at
  which point the fan-out savings would actually be visible end-to-end.

Not pursued now:

- Refactoring `CustomerRepo`/`OrderRepo`/`InvoiceRepo` (and every other
  call site, not just `search.rs`) to borrow a shared `&Surreal<Any>`
  instead of owning a session, which would let one `for_tenant()` session
  serve all three repos in a single omnibox request. This is a real
  signature/lifetime change across the repo trait layer, not a one-line
  fix, and the underlying cause is a workaround for an upstream SDK bug
  (ADR 0002) rather than a design flaw in this codebase. (A narrower
  variant that skips this signature change — one `search_all()` bypassing
  the repo traits instead — was tried and reverted; see above.)
- Isolating which of customer search's four FULLTEXT fields dominates its
  standalone ~228ms p95 (e.g. by timing each field's condition alone via
  `EXPLAIN`/direct `/sql` calls, the same method used in ADR 0003). No
  evidence yet on whether this is one slow field, four merely-additive
  ones, or something else — worth investigating if customer search
  latency matters on its own, not as part of this ADR.

Both are worth revisiting if `/api/search` or `/api/customers?q=` latency
becomes a real problem, not preemptively.

## Consequences

- `docs/perf.md` documents actual p95 for the omnibox and both per-entity
  endpoints against the <100ms target, with this ADR as the explanation,
  per PLAN.md's rule to record plan/reality deviations rather than
  silently redesign.
- Cutting two of the three `for_tenant()` round-trips (via `search_all()`,
  tried above) does measurably shrink the fan-out overhead — the omnibox
  now outpaces a solo customer search instead of trailing it — but does
  **not** close most of the omnibox-specific gap the way this ADR
  originally predicted. Customer search's own per-query cost dominates the
  end-to-end number enough that the fan-out savings are invisible in
  absolute p95. Revise that expectation: an SDK upgrade removing ADR
  0002's clone-depth workaround would still be worth re-measuring, but
  shouldn't be assumed to move the omnibox anywhere near the 100ms target
  on its own.
- The repo-borrowing refactor and the per-field customer-search
  investigation described above remain the fallback fixes if the SDK bug
  isn't resolved and this latency later matters in practice. The per-field
  investigation is now the more promising of the two, given the above.
