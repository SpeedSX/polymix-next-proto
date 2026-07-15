# 0011 — Drop customer numbering entirely

## Status

Accepted.

## Context

M5.1 (`docs/customers-crm.md`) gave `customer` a `number` field —
`CUS-000123`-style, via a per-tenant `customer_prefix` setting — mirroring
how `order.number`/`invoice.number` work, and put it on the same
`autocomplete` edge-ngram FTS index every other customer field uses.

That decision surfaced a real bug:
[0010-customer-number-shares-fts-tokens-with-low-counters.md](0010-customer-number-shares-fts-tokens-with-low-counters.md)
documents zero-padded low counters (`"000001"`, `"000002"`, …) sharing
enough leading edge-ngram tokens that searching an order/invoice number in
a low-volume tenant could also surface an unrelated customer. 0010 accepted
that as cosmetic — the same class of limitation as "no typo tolerance" —
and only adjusted a test's assertion.

Revisiting it: unlike `order.number`/`invoice.number`, `customer.number` has
no external contract to serve.

- An order and an invoice are documents — the number *is* how a customer,
  supplier, or accountant refers to that document outside the system (on a
  PO, an email, a bank transfer reference). That's a real numbering
  requirement.
- A customer isn't a document. Nobody outside PolyMix refers to "customer
  000001" — staff look customers up by name, ЄДРПОУ, contact name, or the
  order/invoice they're tied to. The field existed only because order and
  invoice happened to have one, not because the customer entity needed it.

So `customer.number` bought nothing but cost real things: the ADR-0010
collision, a per-tenant `customer_prefix` setting with no admin surface,
a startup backfill pass (`customer_repo::backfill_numbers`) for legacy rows,
a fifth FTS index to maintain, and a list column nobody was asking to sort
or filter by.

## Decision

Remove `customer.number`, `Tenant.customer_prefix`, the
`customer_search_number` FTS index, and every code path that assigned,
backfilled, displayed, or searched a customer number. No replacement field.

- `migrations/0009_customers_crm.surql` is edited to stop defining the
  index for new tenants (safe: `apply_migrations` tracks progress by
  version number, not content hash — see `migrations.rs`).
  `migrations/0010_remove_customer_number.surql` cleans up tenants that
  already ran the original 0009: drops the index and unsets any `number`
  values that were assigned before this change.
- `docs/customers-crm.md` is updated in place to match — it is normative
  for the customer entity, so it shouldn't describe a field that no longer
  exists.

## Consequences

- The ADR-0010 collision is moot for customers: `customer` is no longer in
  the set of tables a numeric query can match. `omnibox_matches_order_and_invoice_hits`
  in `crates/api/tests/search.rs` now asserts the customer results are
  empty for a shared low-counter query, instead of only asserting
  order/invoice ranking.
- Any tenant's pre-existing customer rows that already had a `number`
  (assigned by the now-removed backfill) lose it via migration 0010; no
  UI or API surface reads it anymore.
- If a future requirement genuinely needs a customer-facing reference
  number (e.g. a loyalty/account number sent to the customer), it should
  be scoped and specced fresh against that requirement rather than
  resurrecting this field — it would likely have different shape and
  uniqueness rules than a simple per-tenant counter.
