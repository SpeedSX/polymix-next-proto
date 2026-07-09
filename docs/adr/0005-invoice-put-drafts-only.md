# 0005 — Invoice `PUT` edits drafts only

## Status

Accepted.

## Context

PLAN.md's API contract table lists `PUT /api/invoices/{id}` under "same
five routes" as customers/orders — i.e. a full update — and the data
model describes invoice `line_items` as "copied from the order at
creation, then independent," implying they can diverge from the order
afterwards. The initial M2 implementation instead made `update()`
unconditionally return `409 conflict` ("void and reissue instead") for
every invoice regardless of status, which is a silent redesign away from
the plan's text rather than a recorded deviation.

Unconditional rejection is right for one case and wrong for another:

- Once an invoice is `issued` (or `paid`/`void`), its totals have been
  sent to the customer and, per the post-prototype roadmap (B2 — invoice
  immutability, GoBD compliance), must never change in place. `409` here
  matches both the plan's spirit and future compliance needs.
- While an invoice is still `draft`, there is no such constraint — it
  hasn't been sent anywhere. Rejecting `PUT` here just removes the
  "catch a mistake before issuing" path the plan's full-update route
  implies, with no compensating benefit.

## Decision

`PUT /api/invoices/{id}` edits the invoice's line items only while
`status == draft`; the service recomputes `net_total`, `tax_total`, and
`gross_total` from the submitted line items (`tax_rate_bp` and currency
are unchanged). Once `issued`, `paid`, or `void`, `PUT` returns `409
conflict` ("invoice can only be edited while in draft status; void and
reissue instead") — unchanged from the original behavior for that part of
the status space.

`order_id`, `customer_id`, `currency`, `exchange_rate`, `tax_rate_bp`,
`issue_date`, and `due_date` are not part of the `PUT` body — they're set
at creation (or at issuance) and are not meant to be edited independently
of those flows.

See `domain::invoice::can_edit` and `SurrealInvoiceRepo::update` in
`backend/crates/surreal-store/src/invoice_repo.rs`.

## Consequences

- Matches the plan's "full update" + "line items ... then independent"
  text for the only period where editing them is safe (draft).
- Preserves invoice immutability once issued, ahead of schedule relative
  to the post-prototype B2 hardening item, at no extra cost.
- Frontend gains a line-item edit form for draft invoices, gated on
  `status === 'draft'`, mirroring the existing order edit form.
- `DELETE /api/invoices/{id}` is unaffected by this ADR — it stays
  unconditionally `409` ("invoices are never deleted — void them") per
  PLAN.md's explicit rule, which is not in tension with the plan's other
  text the way `PUT` was.
