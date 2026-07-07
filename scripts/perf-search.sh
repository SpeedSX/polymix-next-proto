#!/usr/bin/env bash
# Quick p95 measurement for the search endpoints against the seeded demo
# tenant: the /api/search omnibox plus per-entity search-as-you-type on
# /api/customers and /api/orders (both take the same `q` param).
# PLAN.md M3 "Done when": p95 < 100ms on the seeded volume, recorded in /docs/perf.md.
set -euo pipefail

API_URL="${API_URL:-http://127.0.0.1:8080}"
ORG_ID="${ORG_ID:-demo}"
REQUESTS_PER_TERM="${REQUESTS_PER_TERM:-30}"
# Selective 3-letter prefixes of real seeded company names (PLAN.md's "ada"
# style) — NOT generic bigrams like "co": those match almost every row via
# email addresses (all end in ".com", and edge-ngram indexes "co" as a valid
# prefix-token of "com"), turning a prefix search into a full-table rank.
TERMS=("gre" "sti" "kie" "gib" "run" "bau" "kul" "abs" "mos" "lab")

token=$(curl -s -X POST "$API_URL/dev/token" \
  -H 'Content-Type: application/json' \
  -d "{\"user_id\":\"perf-script\",\"org_id\":\"$ORG_ID\"}" | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')

if [ -z "$token" ]; then
  echo "failed to obtain dev token" >&2
  exit 1
fi

sorted_file=$(mktemp)
trap 'rm -f "$sorted_file"' EXIT

measure() {
  local label="$1" path="$2"
  local samples_file
  samples_file=$(mktemp)

  for term in "${TERMS[@]}"; do
    for _ in $(seq 1 "$REQUESTS_PER_TERM"); do
      curl -s -m 10 -o /dev/null -w "%{time_total}\n" \
        -H "Authorization: Bearer $token" \
        --get --data-urlencode "q=$term" \
        "$API_URL$path" >> "$samples_file"
    done
    echo "  done: $term ($(wc -l < "$samples_file") samples so far)" >&2
  done

  local total
  total=$(wc -l < "$samples_file")
  sort -n "$samples_file" > "$sorted_file"
  rm -f "$samples_file"

  percentile() {
    local pct="$1"
    local idx
    idx=$(awk -v n="$total" -v p="$pct" 'BEGIN { i = int(n * p / 100); if (i < 1) i = 1; if (i > n) i = n; print i }')
    awk -v i="$idx" 'NR==i { print $0 * 1000 }' "$sorted_file"
  }

  echo "== $label =="
  echo "requests: $total"
  echo "p50: $(percentile 50) ms"
  echo "p95: $(percentile 95) ms"
  echo "p99: $(percentile 99) ms"
  echo
}

echo "omnibox search (/api/search):" >&2
measure "omnibox search (/api/search)" "/api/search"

echo "customer search (/api/customers?q=):" >&2
measure "customer search (/api/customers?q=)" "/api/customers"

echo "order search (/api/orders?q=):" >&2
measure "order search (/api/orders?q=)" "/api/orders"
