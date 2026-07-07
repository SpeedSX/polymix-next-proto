# `/api/search` perf measurement (M3)

Measured with `scripts/perf-search.sh` against the seeded demo tenant
(50,000 customers, 200,000 orders — see `justfile`'s `seed` recipe). 10
search terms × 30 requests each = 300 samples per run.

PLAN.md's M3 "Done when" target: **p95 < 100ms**.

## Results

### Omnibox (`GET /api/search`)

| Build   | p50     | p95     | p99     |
| ------- | ------- | ------- | ------- |
| debug   | 149ms   | 216ms   | 285ms   |
| debug   | 163ms   | 256ms   | 323ms   |
| debug   | 182ms   | 304ms   | 361ms   |
| release | 166ms   | 291ms   | 420ms   |
| release | 168ms   | 254ms   | 305ms   |

Target **not met** — p95 lands at roughly 2-3x the 100ms target across
both debug and release builds.

### Per-entity search-as-you-type (debug, 300 requests each)

| Endpoint               | p50     | p95     | p99     | vs. 100ms target |
| ----------------------- | ------- | ------- | ------- | ----------------- |
| `GET /api/customers?q=` | 123ms   | 228ms   | 297ms   | not met           |
| `GET /api/orders?q=`    | 37ms    | 56ms    | 63ms    | **met**           |

Order search meets the target on its own. Customer search alone accounts
for most of the omnibox's latency — its p95 (228ms) is close to the
omnibox's own p95 (304ms), while order search barely moves the total.
Customer search has four FULLTEXT fields (`name`, `contact_name`, `email`,
`address.city`) fanned into one `OR`'d condition/score expression, against
order's two (`number`, `notes` — post-ADR-0003); that's the most likely
source of the gap, though not yet isolated field-by-field.

See `docs/adr/0004-search-p95-exceeds-target.md` for the root cause of the
omnibox's overhead specifically (per-request session-open cost — the same
query against SurrealDB's `/sql` endpoint directly resolves in 30-55ms)
and the decision to accept both as a documented deviation rather than
block M3 on further optimization.

## Fixes already applied to get here

- `docs/adr/0002-surrealdb-session-clone-depth.md` — fixed an indefinite
  hang on every non-empty query (SurrealDB Rust SDK session-clone-depth
  bug).
- `docs/adr/0003-order-search-excludes-line-items.md` — fixed a 4.3s p95
  caused by `line_items[*].description`'s FULLTEXT index lacking
  limit-pushdown on array fields in SurrealDB 3.2.

## Reproducing

```sh
just dev   # separate terminal — starts SurrealDB + the API + frontend
bash scripts/perf-search.sh
```
