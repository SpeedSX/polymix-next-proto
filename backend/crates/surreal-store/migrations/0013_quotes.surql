-- Quote documents + staff estimating (Step 3, docs/staff-quoting.md). Embedded
-- lines, counter-numbered, tenant-scoped — same conventions as `order`.
DEFINE TABLE IF NOT EXISTS quote SCHEMALESS;

-- Infix number search (reuses the ngram analyzer from 0006) + notes search.
DEFINE INDEX IF NOT EXISTS quote_search_number ON quote
  FIELDS number FULLTEXT ANALYZER number_ngram BM25 HIGHLIGHTS;
DEFINE INDEX IF NOT EXISTS quote_search_notes ON quote
  FIELDS notes FULLTEXT ANALYZER autocomplete BM25 HIGHLIGHTS;

DEFINE INDEX IF NOT EXISTS quote_status ON quote FIELDS status;
