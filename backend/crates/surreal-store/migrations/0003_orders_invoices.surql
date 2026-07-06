-- Order, invoice, and per-tenant numbering counters (M2). `order` is
-- backtick-escaped since it collides with the SurrealQL ORDER BY keyword.
DEFINE TABLE IF NOT EXISTS `order` SCHEMALESS;
DEFINE TABLE IF NOT EXISTS invoice SCHEMALESS;
DEFINE TABLE IF NOT EXISTS counter SCHEMALESS;
