-- M5.1: Customers CRM profile — extends the customer entity with legal
-- identification (ЄДРПОУ/РНОКПП/ІПН ПДВ), classification & lifecycle
-- status, multiple contacts, and commercial terms. See
-- docs/customers-crm.md, which is normative for this shape.
--
-- Deviates from that doc's literal DDL in one respect: SurrealDB 3.2's
-- FULLTEXT index only accepts a single field (see
-- docs/adr/0001-surrealdb-fulltext-keyword.md), so this migration defines
-- one per-field index instead of the doc's single multi-field
-- `customer_search` index — the same pattern migrations/0004_search.surql
-- already established for this table's older fields. `number` backfill for
-- legacy rows is likewise handled by Rust (`customer_repo::backfill_numbers`,
-- run once at startup alongside migrations) rather than inline SurrealQL,
-- since there's no zero-pad string builtin to lean on here.

-- 1. Scalar backfill. Guarded on `kind`, the first field this migration
--    introduces: legacy rows have it as NONE, already-migrated rows keep
--    whatever they were set to since (idempotent).
-- RETURN NONE throughout this migration: the default UPDATE response echoes
-- every mutated record, which against a large tenant is a payload big enough
-- to blow past the WS transport's message-size limit and reset the
-- connection mid-migration (see 0008_order_status_int.surql).
UPDATE customer SET
  kind = 0,
  status = 1,
  tags = [],
  payment_terms_days = 0,
  default_discount_bp = 0
WHERE kind IS NONE
RETURN NONE;

-- 2. Contacts: fold the legacy scalar contact fields into the new array.
--    Guarded naturally — already-migrated rows have no contact_name/email/
--    phone left to match once step 2b runs.
UPDATE customer SET
  contacts = [{
    name: contact_name OR email OR phone OR "",
    role: NONE,
    email: email,
    phone: phone,
    is_primary: true
  }]
WHERE contact_name IS NOT NONE OR email IS NOT NONE OR phone IS NOT NONE
RETURN NONE;

UPDATE customer SET contacts = [] WHERE contacts IS NONE RETURN NONE;

UPDATE customer UNSET contact_name, email, phone RETURN NONE;

-- 3. Addresses: the legacy single address becomes the legal address;
--    delivery starts empty until staff fill it in.
UPDATE customer SET legal_address = address
  WHERE address IS NOT NONE AND legal_address IS NONE
  RETURN NONE;
UPDATE customer SET delivery_address = NONE WHERE delivery_address IS NONE RETURN NONE;
UPDATE customer UNSET address RETURN NONE;

-- 4. Full-text search: drop the indexes on the now-removed contact_name/
--    email columns; add indexes for the new identity/contact fields.
--    `name`'s index is unchanged from migrations/0004_search.surql.
REMOVE INDEX IF EXISTS customer_search_contact_name ON customer;
REMOVE INDEX IF EXISTS customer_search_email ON customer;

DEFINE INDEX IF NOT EXISTS customer_search_legal_name ON customer
  FIELDS legal_name FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS customer_search_number ON customer
  FIELDS number FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS customer_search_edrpou ON customer
  FIELDS edrpou FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
-- Array-field FULLTEXT indexes don't get the scalar-field limit pushdown
-- (see docs/adr/0003-order-search-excludes-line-items.md) — acceptable here
-- because each customer has only 1-3 contacts, a much lower-cardinality
-- vocabulary than order line items' fixed product-name set. Re-verify
-- against docs/perf.md in Step 6; drop customer_search_contacts_email
-- first if the budget is blown (least demo value).
DEFINE INDEX IF NOT EXISTS customer_search_contacts_name ON customer
  FIELDS contacts[*].name FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS customer_search_contacts_email ON customer
  FIELDS contacts[*].email FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
