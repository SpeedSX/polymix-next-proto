---
name: perf-check
description: Run the /api/search perf benchmark (scripts/perf-search.sh) against the seeded demo tenant in one shot — brings up SurrealDB, verifies/seeds the demo tenant, starts the API, runs the benchmark, and reports p50/p95/p99 for the omnibox and per-entity search endpoints. Use when asked to run/check/measure search perf, or to re-measure after a change touching /api/search, /api/customers, or /api/orders.
---

# Perf check: `/api/search`

One-shot procedure for measuring search latency against the seeded demo tenant. Mirrors what `docs/perf.md` and
`docs/adr/0004-search-p95-exceeds-target.md` already record — the goal here
is repeating that measurement cheaply, not re-deriving the steps each time.

## 0. Container runtime

Prefer `docker` if present, else `podman` (see root `CLAUDE.md` for why
`docker` may be absent on this machine and how to confirm `podman` is
running). This step only needs the CLI — no `DOCKER_HOST` override needed
here; that override is a separate concern for the Rust `testcontainers`
integration tests, not for this compose-based setup.

```
command -v docker >/dev/null 2>&1 && RUNTIME=docker || RUNTIME=podman
```

## 1. Bring up SurrealDB

Idempotent — safe to always run, even if already up:

```
$RUNTIME compose -f deploy/compose.yaml up -d --wait
```

## 2. Check the demo tenant is seeded

```
token=$(curl -s -X POST http://127.0.0.1:8080/dev/token \
  -H 'Content-Type: application/json' -d '{"user_id":"perf-check","org_id":"demo"}' \
  | sed -n 's/.*"token":"\([^"]*\)".*/\1/p')
```

This needs the API already up — if step 3 hasn't run yet, do that first,
then come back to check seeding via:

```
curl -s -H "Authorization: Bearer $token" "http://127.0.0.1:8080/api/customers?limit=1"
```

Look at the `total` field. If it's `0` (fresh volume), seed it first —
`just seed` from repo root (needs only the DB up, not the API; it connects
to SurrealDB directly). Expect ~10000 customers / ~100000 orders; takes a
couple of minutes.

## 3. Start the API

Check first: `curl -s -m 3 http://127.0.0.1:8080/api/health` — if it
already responds `{"status":"ok"}`, skip this step.

Otherwise start it **as a properly tracked background task**, not a
shell-level fork:

```
cd backend && AUTH_DEV_MODE=true PORT=8080 cargo run -p api
```

Run this via the Bash tool's own `run_in_background: true` parameter.
**Do not** wrap it yourself in `(cmd &)`/`nohup`/similar — on this
Windows + git-bash setup that produces an orphaned process the harness
loses track of (it keeps running, holds port 8080 and the build lock, and
has to be hunted down manually via `Get-CimInstance Win32_Process -Filter
"Name='api.exe'"` and killed by PID). Using the tool's native
backgrounding avoids that.

Poll `http://127.0.0.1:8080/api/health` (a few retries, ~1s apart) until
it responds before moving on.

## 4. Run the benchmark

```
bash scripts/perf-search.sh
```

Takes about a minute (300 requests × 3 endpoints). Run it 2-3 times if the
numbers matter for a decision (single runs are noisy — see ADR 0004's own
methodology) and report the range, not just one sample.

## 5. Report, don't auto-document

Report p50/p95/p99 for all three endpoints (omnibox, customers, orders)
against PLAN.md's <100ms target.

- **Comparing against `docs/perf.md`'s recorded numbers:** only trust
  same-run, same-machine comparisons at face value (e.g. omnibox vs. a
  solo customer-search in *this* run). Absolute numbers against a prior
  recorded baseline may reflect a different machine, not a regression or
  improvement — flag that caveat rather than asserting a delta.
- **Don't edit `docs/perf.md` or the ADR files** with the new numbers
  without asking first — this project iterates on those docs directly and
  the last several sessions were mid-edit on them.

## Cleanup

Leaving SurrealDB (and the API, if you started it) running is fine and
usually preferable — cheap to leave up for the next session. Only tear
down if asked:

```
$RUNTIME compose -f deploy/compose.yaml down
```

(and stop the `cargo run -p api` background task / kill its PID if you
started one).
