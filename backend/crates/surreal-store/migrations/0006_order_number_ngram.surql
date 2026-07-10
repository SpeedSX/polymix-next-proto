-- Order numbers (ORD-NNNNNN) only ever need infix search, not general
-- search-as-you-type: the `autocomplete` edge-ngram analyzer (0004) only
-- indexes prefixes of each token, so a query landing mid-number (e.g.
-- "2345" inside "ORD-023451") never matches — see
-- docs/adr/0009-order-number-infix-search.md. Switch this one field to a
-- true (non-anchored) ngram analyzer instead.
--
-- min=3 rather than min=2: for a 6-digit number token this drops from 15 to
-- 10 generated ngrams (~33%), and for the constant "ord" token it drops the
-- 2-grams "or"/"rd" entirely — those match every row regardless of number,
-- so they cost index space for zero discriminating power. Net effect:
-- 3-character minimum query length on this field, ~39% fewer generated
-- tokens overall. Verified live against the pinned surrealdb:v3.2 image
-- that DEFINE INDEX OVERWRITE + REBUILD INDEX correctly reprocesses
-- already-indexed rows (INFO FOR INDEX showed status "ready" over existing
-- data, not just newly inserted rows).
DEFINE ANALYZER IF NOT EXISTS number_ngram TOKENIZERS class FILTERS lowercase, ascii, ngram(3, 10);

DEFINE INDEX OVERWRITE order_search_number ON `order`
  FIELDS number FULLTEXT ANALYZER number_ngram BM25 HIGHLIGHTS;
REBUILD INDEX order_search_number ON `order`;
