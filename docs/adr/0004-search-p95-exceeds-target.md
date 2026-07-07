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

Not pursued now:

- Refactoring `CustomerRepo`/`OrderRepo`/`InvoiceRepo` (and every other
  call site, not just `search.rs`) to borrow a shared `&Surreal<Any>`
  instead of owning a session, which would let one `for_tenant()` session
  serve all three repos in a single omnibox request. This is a real
  signature/lifetime change across the repo trait layer, not a one-line
  fix, and the underlying cause is a workaround for an upstream SDK bug
  (ADR 0002) rather than a design flaw in this codebase.
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
- If the SurrealDB Rust SDK is upgraded and ADR 0002's clone-depth
  workaround can be removed (sharing one cloned session across repos
  again), re-measure the omnibox — that alone would cut two of the three
  `for_tenant()` round-trips and likely close most of the omnibox-specific
  gap, though not customer search's own standalone latency.
- The repo-borrowing refactor and the per-field customer-search
  investigation described above remain the fallback fixes if the SDK bug
  isn't resolved and this latency later matters in practice.
