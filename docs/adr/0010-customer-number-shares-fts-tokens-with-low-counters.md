# 0010 — Zero-padded low counter values share edge-ngram tokens across entities

## Status

Superseded by
[0011-drop-customer-numbering.md](0011-drop-customer-numbering.md), which
removes `customer.number` (and its FTS index) entirely rather than living
with the collision this ADR accepted. Kept for the record of why the
collision happened and why living with it was the original call.

## Context

`docs/customers-crm.md`'s M5.1 FTS delta adds `customer.number` to the
indexed/searchable fields, on the same `autocomplete` (edge-ngram) analyzer
every other customer field already uses. Discovered while running
`crates/api/tests/search.rs`'s `omnibox_matches_order_and_invoice_hits`
against a real SurrealDB (`cargo test --workspace -- --ignored`): the test
creates one customer, one order, and one invoice in a fresh tenant, and
asserts that searching the order/invoice's shared number (`"000001"`, since
counters are independent per-tenant and both start at 1) returns that order
and invoice but no customers. It failed — the customer (whose own,
independently-counted number also starts from `"000001"`-ish territory) showed
up in the customer results too.

Root cause, per `docs/adr/0009-order-number-infix-search.md`'s own documented
matching mechanism ("a query is matched by re-running it through the same
analyzer and intersecting"): `edgengram(2, 10)` tokenizes a 6-digit
zero-padded counter into every prefix from length 2 up, anchored at position
0. For `"000001"` that's `{00, 000, 0000, 00000, 000001}`; for `"000002"` it's
`{00, 000, 0000, 00000, 000002}`. The intersection — `{00, 000, 0000,
00000}` — is non-empty, so the `@N@` predicate reports a match. This isn't a
planner bug or a spec mismatch; it's the correct, documented behavior of
prefix/edge-ngram matching applied to values that are mostly leading zeros.
It affects **any** two zero-padded counters under ~100000 (nearly all test
and small-tenant data), and would equally affect order.number and
invoice.number if two of those ever needed to be told apart in one query —
it just hadn't surfaced before because the omnibox's per-entity-type queries
never compared numbers *across* table types until customer.number joined the
indexed set.

## Decision

Don't change the analyzer or add a minimum-match-length special case — this
is the same class of accepted limitation ADR-0001 already documents ("no
typo tolerance, no mid-word match"), just one token-sharing arithmetic away
from being visible. A short numeric query like `"000001"` is inherently
ambiguous under prefix search once *any* other zero-padded field shares
enough leading digits; ranking (BM25, summed per matched field) still pushes
a true full-string match to the top, which is what the product actually
promises — "search-as-you-type", not "exact match, single hit".

Instead: `omnibox_matches_order_and_invoice_hits` no longer asserts the
customer side is empty. Its job is to pin order/invoice highlighting and
ranking, not the customer table's absence, which was only ever incidentally
true before `customer.number` became searchable.

## Consequences

- Searching a short, mostly-zero numeric string (an early counter value in
  any low-volume tenant) can surface unrelated customers, orders, or
  invoices whose own numbers share the same leading digits. This is
  cosmetic in practice: real tenants accumulate enough volume that leading
  digits stop being the whole story, and BM25 ranking still favors the
  true match.
- If this becomes a real user complaint (unlikely — same shape as the
  already-accepted "no typo tolerance" limitation), the fix is the same
  pattern as ADR-0009: a per-field analyzer change, evaluated against actual
  reported behavior, not pre-emptively here.
- No test in this suite should assert a cross-entity-type absence based on
  a short numeric query in a low-counter tenant; assert on presence/ranking
  of the intended hit instead (what this fix does).
