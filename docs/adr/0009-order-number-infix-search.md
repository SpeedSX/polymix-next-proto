# 0009 — Order-number search misses mid-number substrings under edge-ngram

## Status

Accepted.

## Context

Reported behavior: searching orders (both the per-entity order list and the
global omnibox) for a substring like `2345` inside an order number such as
`ORD-023451` finds nothing, while `0234` or `02345` (both leading substrings
of the digit run) do.

Root cause is `migrations/0004_search.surql`'s `autocomplete` analyzer:

```
DEFINE ANALYZER IF NOT EXISTS autocomplete TOKENIZERS class FILTERS lowercase, ascii, edgengram(2, 10);
```

`edgengram(min, max)` only ever generates **prefixes** of a token, anchored
at position 0 — for the digit token `023451` the indexed set is exactly
`{02, 023, 0234, 02345, 023451}`. A query is matched by re-running it through
the same analyzer and intersecting; `2345`'s own edge-ngrams (`23, 234,
2345`) never appear in that set because they start at offset 1, not offset
0. `0234`/`02345` happen to match because they *are* literal members of the
prefix set. This is expected edge-ngram behavior — it implements
autocomplete/prefix search, not infix/substring search — and matches what
ADR 0001 already documented about the `autocomplete` analyzer only ever
indexing prefixes (§ "exact-prefix beats mid-word").

Confirmed against the SurrealDB docs (`DEFINE ANALYZER`) that this is
inherent to `edgengram` — there is no suffix- or middle-anchored variant.
SurrealDB does have a second filter, `ngram(min, max)`, which generates
every substring in the min..max length range from any offset (prefix,
middle, and suffix), at the cost of a larger index than edge-ngram for the
same token.

Order numbers are the right field to fix this way: they're short,
fixed-format (`{order_prefix}-{6-digit counter}` or just the 6-digit counter
when `order_prefix` is empty, the M4 default), and the discriminating part
of a user's search is often a substring copied from the middle of the
number, not the start.

## Decision

Give `order.number` its own `ngram(3, 10)` analyzer, `number_ngram`, and
leave every other FULLTEXT field (customer name/contact/email/city, order
notes, invoice number) on the `autocomplete` edge-ngram analyzer.

- `min=3`, not `2`. For a 6-digit counter token, `ngram(2,10)` generates 15
  tokens (`Σ(6-n+1)` for `n=2..6`) vs. 10 for `ngram(3,10)` — a ~33%
  reduction for that token. For an alpha prefix token like `ord` (from a
  configured `order_prefix`), `ngram(2,10)` additionally indexes `or`/`rd`
  as 2-grams — but every order sharing that prefix produces the *same*
  `or`/`rd` tokens, so they sit in a postings list matching every row with
  zero discriminating power. `min=3` drops that dead weight entirely.
  Net: ~39% fewer generated tokens per order for this field (18 → 11,
  summed across both tokens), at the cost of a 3-character minimum query
  length on this field specifically (a 1-2 character query now returns no
  matches from `order.number`, same as it would from any other FULLTEXT
  field's ranking floor).
- Migration mechanism verified live against the pinned `surrealdb:v3.2`
  image (matching ADR 0001's verify-don't-assume approach) before writing
  it: `DEFINE INDEX OVERWRITE <name> ... FULLTEXT ANALYZER <new> BM25
  HIGHLIGHTS` followed by `REBUILD INDEX <name> ON <table>` correctly
  reprocesses rows that existed under the old analyzer — `INFO FOR INDEX`
  reported `{"initial": 2, "pending": 0, "status": "ready", "updated": 0}`
  against two pre-existing rows, and a query that only matched under the
  new analyzer (`2345`-style mid-substring) returned the row afterward.
  `migrations/0006_order_number_ngram.surql` applies this in place; `0004`
  keeps the original `DEFINE INDEX IF NOT EXISTS ... autocomplete` statement
  unchanged (migrations are immutable once numbered) with a comment pointing
  forward to 0006.

## Consequences

- Order-number search (both `GET /api/orders?q=` and the `/api/search`
  omnibox) now matches any 3+ character substring of the number, not just a
  leading substring — fixes the reported behavior.
- Order notes, and every customer/invoice FULLTEXT field, are unaffected —
  still edge-ngram, still prefix-only, still subject to the same "no typo
  tolerance, no mid-word match" limitation ADR 0001/PLAN.md's Risks section
  already documents. If the same complaint comes in for another field
  (invoice number is the obvious next candidate, same short-fixed-format
  shape), the fix is the same pattern: a per-field `ngram` analyzer, not a
  blanket switch of `autocomplete` itself — customer name/contact/email/city
  are free-text and would balloon in index size under full `ngram` given
  their length and volume.
- `crates/api/tests/search.rs`'s
  `order_number_search_matches_mid_number_substring` pins both the fixed
  mid-substring match and the new 3-character floor.
