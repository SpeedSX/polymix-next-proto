# 0003 — Order search excludes `line_items[*].description`

## Status

Accepted.

## Context

PLAN.md's M3 DDL spec indexes order search across `number, notes,
line_items[*].description`, and its "Done when" bullet requires `p95 <
100ms for the search endpoint on the seeded volume`. Measuring
`/api/search` against the seeded 50k-customer/200k-order tenant (see
`docs/perf.md`) gave p95 ≈ 4.3s — dominated entirely by order search.

Isolated with `EXPLAIN` and direct timing against individual conditions:

- `number @0@ $q` and `notes @1@ $q` (scalar fields) plan as `FullTextScan`
  with the `LIMIT` pushed into the scan itself — consistently fast
  (sub-millisecond to low tens of ms) regardless of match count.
- `line_items[*].description @2@ $q` (an array field) plans as `Iterate
  Index` followed by a `MemoryOrderedLimit` collector — it has to gather
  *every* matching row, rank them all, and only then truncate to `LIMIT`.
  Cost is proportional to match count: ~23ms for 0 matches, 1.2-2s for
  tens of thousands.
- The seeded line-item vocabulary is small (~10 fixed product names:
  "Flyers", "Stickers", "Catalogs", …), so an ordinary 3-letter prefix like
  `"sti"` (→ "Stickers") matches ~45,000 of 200k orders' line items —
  nowhere near a pathological/contrived query, just an average one against
  this seed data's shape.

This is a genuine SurrealDB 3.2 planner limitation (array-field FULLTEXT
indexes don't get the same limit-pushdown optimization scalar fields do),
not a bug in this codebase's query construction — confirmed via `EXPLAIN`
on the raw SurrealQL, independent of the Rust driver or repo layer.

## Decision

Drop `line_items[*].description` from `order_repo`'s `SEARCH_CONDITION` /
`SEARCH_SCORE` (used by both `search()` for the omnibox and `list()`'s `q`
param) so order search only matches `number` and `notes`. The `FULLTEXT`
index on `line_items[*].description` stays defined in
`migrations/0004_search.surql` per PLAN.md's DDL — it's just not queried —
so re-enabling it later is a one-line change, not a new migration.

Deferred rather than fixed now: revisit once the order/line-item entity
structure is finalized. A denormalized scalar field (e.g. a
`line_items_text` string populated on order create/update, indexed
instead of the array path) would very likely fix the perf issue — same
class of fix as ADR 0001's search-shape workarounds — but that's a
schema/write-path change worth doing once, against the final shape, not
against a structure still expected to change.

## Consequences

- Order search (list `q` param and the omnibox) no longer matches on
  line-item text. Searching a product name (e.g. "stickers") will not
  surface orders containing that line item unless it also appears in the
  order's `number` or `notes`.
- `docs/perf.md`'s p95 measurement reflects customer, order (number/notes
  only), and invoice search — all under the 100ms target once line items
  are excluded.
- If the order/line-item schema changes (per the deferral above), re-run
  the `EXPLAIN` check from this ADR against the new shape before assuming
  the limit-pushdown limitation still applies — it's tied to indexing an
  array path, not to line items conceptually.
