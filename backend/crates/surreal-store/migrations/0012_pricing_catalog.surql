-- Pricing catalog (A2a): the five PriceModel tables plus the per-tenant
-- pricelist version record (docs/quote-engine-spec.md §2).
--
-- SCHEMALESS because the closed field sets carry tagged unions and open maps
-- (`material.pricing`, the digital/offset machine cost split, `attrs`) that a
-- schemafull DEFINE FIELD can't express cleanly; the quote-engine structs are
-- the normative shape and validate on read. Defined eagerly (not lazily on
-- first CREATE) because the catalog is listed before it is ever written on a
-- fresh tenant, and SurrealDB errors on SELECT against an undefined table.
DEFINE TABLE IF NOT EXISTS format SCHEMALESS;
DEFINE TABLE IF NOT EXISTS material SCHEMALESS;
DEFINE TABLE IF NOT EXISTS machine SCHEMALESS;
DEFINE TABLE IF NOT EXISTS operation SCHEMALESS;
DEFINE TABLE IF NOT EXISTS pricing_policy SCHEMALESS;

-- Single version record; every catalog mutation bumps it in the same
-- transaction (spec §2). Initialized to 1 so a fresh tenant's first snapshot
-- and the golden fixture agree (§9.6 pricelist_version: 1). Idempotent: a
-- re-run keeps an already-incremented version rather than resetting it.
UPSERT meta:pricing SET version = version ?? 1 RETURN NONE;
