# 0006 — Meeting the search p95 target: tenant session cache + per-field customer search

## Status

Accepted. Supersedes ADR 0004's decision to accept the missed target.

## Context

ADR 0004 accepted `/api/search` p95 at 2.5-3x PLAN.md's 100ms target after
transport swaps (HTTP engine) and fan-out consolidation (`search_all()`)
failed to move the number. What was missing was a phase-by-phase
decomposition of where a request's milliseconds actually go.
`crates/surreal-store/examples/perf_probe.rs` (run against the seeded
50k-customer tenant, release build) provided it:

| Phase | p50 | p95 |
| --- | --- | --- |
| `for_tenant()` session open | 25.6ms | 41.8ms |
| customer 4-field OR hit query, warm session | 53.5ms | 128.0ms |
| customer count subquery (unbounded OR), warm | 39.4ms | 75.6ms |
| each customer field queried **alone**, warm | 1.3-9.7ms | 14.0-16.1ms |
| order 2-field hit query, warm | 1.0ms | 1.4ms |
| fresh session + OR hit query (prod shape) | 91.2ms | 179.1ms |

Two findings:

1. **Session setup is not cheap.** `root.clone()` + `use_ns`/`use_db`
   costs 25-40ms per call on this SDK/server pair, and every authenticated
   request paid it (the omnibox paid it once after the `search_all()`
   consolidation). PLAN.md's "sessions are cheap; do not cache them per
   tenant" assumption is empirically false here.
2. **The multi-index OR is the dominant query cost.** The same four
   predicates cost ~105ms as one `f1 @0@ $q OR f2 @1@ $q OR …` statement
   for a common prefix (confirmed via direct `/sql`), but ~10-20ms each as
   separate single-field statements: SurrealDB 3.2 pushes the LIMIT into a
   single-index FullTextScan but not into a multi-index OR, so the OR form
   scores and sorts the full match set in memory. This is the same planner
   limitation family as ADR 0003's array-field finding.

## Decision

Two changes, verified together by three `scripts/perf-search.sh` runs:

1. **Cache tenant sessions** (`store.rs`): `Store` keeps a
   `moka::future::Cache<String, Arc<Surreal<Any>>>`; `for_tenant()` returns
   the cached `Arc`. Repos now hold `Arc<Surreal<Any>>`. The `Arc` is the
   sharing mechanism — cloning the `Surreal` inside would be a
   second-generation session clone, which hangs queries (ADR 0002).
   Concurrent use of one session across requests is safe because repos only
   issue self-contained statements (no session variables, no interactive
   transactions). Startup migration re-application warms the cache for
   every registered tenant as a side effect.
2. **Split customer search into per-field statements**
   (`customer_repo.rs`): both `search()` and the `q`-filtered `list()` send
   one statement per searchable field (plus, for `list()`, one
   `SELECT VALUE id` per field) in a **single** `.query()` round-trip, then
   merge in Rust:
   - Ranking: per-record scores are **summed** across fields — the same
     combined-relevance semantics as the old
     `(search::score(0) + … + search::score(3))`, pinned by the
     `multi_field_match_outranks_single_field_match` integration test.
   - `total`: exact distinct count from the deduped union of the per-field
     id lists. Per-field `count()` sums over-count multi-field matches, and
     pushing the union into SurrealQL
     (`array::distinct(array::flatten([...subqueries...]))`) loses the
     fast path (~62ms vs ~24ms measured).
   - `search::highlight(..., 0)` per statement now highlights the field
     that actually matched (e.g. a contact name), instead of falling back
     to the label.
   - Deep pagination is capped (`MAX_SEARCH_WINDOW = 1000` rows per field);
     a component score is lost when a record places outside another
     field's window, so ranking near the window edge is approximate.

Order and invoice search are untouched: order's two-field OR measured 1.4ms
p95 on the seeded data and already met the target.

## Results

`scripts/perf-search.sh`, seeded demo tenant, 3 runs, debug build, same
machine for before/after server-side comparisons:

| Endpoint | p95 before (ADR 0004 era) | p95 after (3 runs) | target |
| --- | --- | --- | --- |
| omnibox `/api/search` | 216-304ms | **72 / 84 / 77ms** | <100ms ✅ |
| `/api/customers?q=` | 228-368ms | 93 / 100 / 102ms | <100ms ~✅ (boundary) |
| `/api/orders?q=` | 56ms | 21-25ms | <100ms ✅ |

("Before" numbers span two machines; the server-side query timings that
motivated the split — 105ms OR vs ~40-60ms split, same `/sql` session —
are same-machine and unambiguous.)

## Consequences

- PLAN.md's M3 "p95 < 100ms" acceptance criterion is now met for the
  search endpoint; `docs/perf.md` and ADR 0004 should be updated to point
  here (left to a follow-up edit — those docs were mid-iteration).
- PLAN.md's rule "sessions are cheap; do not cache them per tenant" is
  overridden by this ADR for this SDK/server combination. If the SDK's
  session-open cost drops in a future upgrade, the cache is harmless but
  removable.
- Cached sessions live for the process lifetime (capacity 10k tenants).
  Session state after a SurrealDB reconnect follows the same SDK
  router-replay mechanism as before; the M4 hub work that owns reconnect
  behavior should verify cached sessions survive it (or invalidate the
  cache on reconnect).
- The customers list endpoint sits at the target boundary (p95 93-102ms).
  The remaining cost is the 25-row `SELECT *` payloads and the id-list
  union; if it needs to drop further, windowing the id lists (making
  `total` exact only up to a cap) is the next lever.
- `examples/perf_probe.rs` stays as the measurement harness for any future
  regression in this area; its phase A now measures the cached path.
