-- Full-text search (M3): edge-ngram analyzer for search-as-you-type, plus a
-- BM25 index per searchable field.
--
-- One FULLTEXT index per field, not one multi-field index per table, and
-- FULLTEXT rather than PLAN.md's SEARCH spelling — see
-- docs/adr/0001-surrealdb-fulltext-keyword.md for why (SurrealDB 3.x's
-- FULLTEXT index kind only accepts a single column).
DEFINE ANALYZER IF NOT EXISTS autocomplete TOKENIZERS class FILTERS lowercase, ascii, edgengram(2, 10);

DEFINE INDEX IF NOT EXISTS customer_search_name ON customer
  FIELDS name FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS customer_search_contact_name ON customer
  FIELDS contact_name FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS customer_search_email ON customer
  FIELDS email FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;

-- Superseded by migrations/0006_order_number_ngram.surql, which overwrites
-- this index with an ngram analyzer for infix matching.
DEFINE INDEX IF NOT EXISTS order_search_number ON `order`
  FIELDS number FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS order_search_notes ON `order`
  FIELDS notes FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;

DEFINE INDEX IF NOT EXISTS invoice_search_number ON invoice
  FIELDS number FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;
