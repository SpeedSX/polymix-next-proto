-- customer_search_city and order_search_line_items are dropped: neither is
-- queried (city search was removed from SEARCH_FIELDS; line_items was
-- already unqueried per docs/adr/0003-order-search-excludes-line-items.md,
-- which had kept it defined for later reuse — superseded by the write-cost
-- of maintaining a FULLTEXT/BM25 index on every insert for a field nothing
-- reads). `IF EXISTS` because 0004 no longer defines either for new
-- tenants; this migration only has work to do on tenants provisioned
-- before this change.
REMOVE INDEX IF EXISTS customer_search_city ON customer;
REMOVE INDEX IF EXISTS order_search_line_items ON `order`;
