# 0001 — SurrealDB 3.2's `FULLTEXT` index feature diverges from PLAN.md's spec

## Status

Accepted.

## Context

PLAN.md's M3 spec (SurrealDB definitions section) gives the search index
and query syntax as:

```sql
DEFINE INDEX customer_search ON customer
  FIELDS name, contact_name, email, address.city
  SEARCH ANALYZER autocomplete BM25 HIGHLIGHTS;
```

```sql
SELECT *, search::score(0) AS score FROM customer
WHERE name @0@ $q OR contact_name @0@ $q OR email @0@ $q
ORDER BY score DESC LIMIT $limit;
```

Verified interactively against the pinned `surrealdb/surrealdb:v3.2` server
(the same version the project's `surrealdb` Rust SDK crate is pinned to)
and against the vendored `surrealdb-core-3.2.0` parser source
(`src/syn/parser/stmt/define.rs::parse_define_index`), four things about
this spec don't hold on the installed version:

1. **Keyword is `FULLTEXT`, not `SEARCH`.** `SEARCH` is not a recognized
   keyword at that grammar position at all; parsing fails with `Unexpected
   token, expected Eof`.
2. **One field per `FULLTEXT` index, not a field list.** The parser
   explicitly rejects multiple columns for this index kind
   (`Expected one column, found {n}`). PLAN.md's `FIELDS name,
   contact_name, email, address.city` on one index is invalid; each
   searchable field needs its own index.
3. **Match references must be unique per query, not shared.** Using the
   same `@0@` reference across predicates on *different* indexes (e.g.
   `name @0@ $q OR contact_name @0@ $q` once each field has its own index)
   errors with `Duplicated Match reference: 0`. Each field predicate needs
   a distinct reference number (`@0@`, `@1@`, `@2@`, …), and a combined
   relevance score is the sum of `search::score(N)` across them.
4. **`ORDER BY` needs the expression pre-aliased in the `SELECT` list.**
   `ORDER BY search::score(0) DESC` on its own fails
   (`Missing order idiom`); the query must project it first —
   `SELECT *, (search::score(0) + search::score(1)) AS score ... ORDER BY
   score DESC`.

A fifth, related bug (not a spec mismatch, a planner bug) also surfaced
while building the ranked list/count queries: **`SELECT count() FROM t
WHERE <fulltext predicate> GROUP ALL` silently returns 0** even when rows
match — confirmed by running the identical `WHERE` clause as a plain
`SELECT *` (returns the matching rows) side by side with the `count()`
form (returns `0`). Wrapping the filter in a subquery works around it:
`SELECT count() FROM (SELECT id FROM t WHERE <predicate>) GROUP ALL`
returns the correct count.

None of this affects the `@N@` operator, `search::score`, or
`search::highlight` themselves once each is used per the corrected shape
above — the underlying BM25/highlight behavior matches PLAN.md's intent.

## Decision

Keep PLAN.md's intent (BM25-ranked, highlighted full-text search per
entity, `q`-driven on list endpoints plus a global omnibox) but implement
it against the syntax the installed SurrealDB version actually accepts:

- `crates/surreal-store/migrations/0004_search.surql` defines one
  `FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS` index per searchable
  field (4 for `customer`, 3 for `order`, 1 for `invoice`), not one
  multi-field index per table.
- Repository FTS queries give each field predicate its own match
  reference and sum `search::score(N)` across them, aliased so `ORDER BY`
  can reference it.
- Ranked-list `count()` queries wrap the full-text `WHERE` in a subquery
  to avoid the zero-count planner bug; the existing non-FTS count path is
  unaffected and untouched.

## Consequences

- `migrations/0004_search.surql` has more `DEFINE INDEX` statements than
  PLAN.md's spec shows (one per field instead of one per table), each
  named `{table}_search_{field}`.
- Repository code (`customer_repo`, `order_repo`, `invoice_repo`) builds
  the `WHERE`/`ORDER BY`/count SQL with per-field match references and the
  count subquery workaround, rather than the single-shared-reference form
  PLAN.md's inline example shows.
- The API contract, response shapes, and ranking semantics ("exact-prefix
  beats mid-word", BM25 ordering) are unaffected — this is purely a DDL
  and query-construction correction.
- If SurrealDB is upgraded past 3.2, re-verify all five points against the
  installed version's parser/planner (see `docs/surrealdb-rust-sdk-notes.md`
  §9 for how this was confirmed) before trusting this ADR or PLAN.md's
  original spelling.
