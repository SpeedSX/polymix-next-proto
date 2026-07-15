-- Customer numbering never made product sense — unlike order/invoice
-- numbers (which are the customer-facing reference for a document),
-- customer.number had no external contract and existed only to be listed
-- and searched. Its only externally visible effect was the cross-entity FTS
-- collision documented in docs/adr/0010-customer-number-shares-fts-tokens-with-low-counters.md,
-- superseded by docs/adr/0011-drop-customer-numbering.md, which records the
-- decision to remove it instead of chasing a fix for the collision.
--
-- 0009 no longer defines `customer_search_number` for new tenants (edited
-- retroactively — safe, since apply_migrations tracks progress by version
-- number, not content hash); `IF EXISTS` here only has work to do on
-- tenants that ran the original 0009.
REMOVE INDEX IF EXISTS customer_search_number ON customer;
UPDATE customer UNSET number RETURN NONE;
