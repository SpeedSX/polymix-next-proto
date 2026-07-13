# Falls back to podman when docker isn't on PATH (e.g. local dev on this
# machine) — CI runners have docker, so this is a no-op there.
container_runtime := `command -v docker >/dev/null 2>&1 && echo docker || echo podman`

# Bring up SurrealDB + the API (dev auth mode) + the Vite dev server.
#
# Kills only the two background jobs, not the whole process group: `kill 0`
# signals the group leader (this sh) too, and each subshell inherits the trap
# and re-fires it — the resulting self-signal re-entrancy is what crashes
# MSYS2's runtime into a stackdump on Windows.
dev:
    {{container_runtime}} compose -f deploy/compose.rocksdb.yaml up -d --wait
    trap 'kill $(jobs -p) 2>/dev/null' EXIT INT TERM; \
    (cd backend && AUTH_DEV_MODE=true PORT=8080 SURREALDB_URL=ws://localhost:8001 cargo run -p api) & \
    (cd frontend && npm run dev) & \
    wait

# Same as `dev`, but against the SurrealDB Cloud free-tier instance instead
# of the local docker-compose SurrealDB — no container runtime needed at
# all. Reads backend/.env.cloud.local (gitignored — copy
# backend/.env.cloud.local.example and fill in your instance's
# SURREALDB_USER/SURREALDB_PASS first). See
# docs/adr/0011-surrealdb-hosting-cloud-free-tier-instead-of-fly.md.
dev-cloud:
    test -f backend/.env.cloud.local || { echo "Missing backend/.env.cloud.local — copy backend/.env.cloud.local.example and fill in your SurrealDB Cloud credentials first."; exit 1; }
    set -a; . ./backend/.env.cloud.local; status=$?; set +a; \
    test "$status" -eq 0 || { echo "Failed to load backend/.env.cloud.local"; exit "$status"; }; \
    trap 'kill $(jobs -p) 2>/dev/null' EXIT INT TERM; \
    (cd backend && PORT=8080 cargo run -p api) & \
    (cd frontend && npm run dev) & \
    wait

# fmt --check, clippy -D warnings, cargo test, eslint, tsc --noEmit, vitest.
check:
    cd backend && cargo fmt --check
    cd backend && cargo clippy --workspace --all-targets -- -D warnings
    cd backend && cargo test --workspace
    cd frontend && npm run lint
    cd frontend && npm run typecheck
    cd frontend && npm run test

# Backend integration tests (testcontainers). Cleans up afterwards regardless
# of pass/fail: the `testcontainers` crate has no Ryuk-style reaper, and our
# harness caches the SurrealDB container in a `static OnceCell` per test
# binary for speed — Rust never runs `Drop` on statics at process exit (and
# `cargo test`'s harness calls `std::process::exit` anyway), so containers
# would otherwise leak on every run. Every container it starts carries
# `org.testcontainers.managed-by=testcontainers`, so filter on that.
test-int:
    cd backend && cargo test --workspace -- --ignored; code=$?; \
    ids=$({{container_runtime}} ps -aq --filter "label=org.testcontainers.managed-by=testcontainers"); \
    if [ -n "$ids" ]; then {{container_runtime}} rm -f $ids; fi; \
    exit $code

# Full-scale seeder against the local dev tenant.
seed:
    cd backend && cargo run -p seeder

# Small-scale seeder against the Ukrainian demo tenant,
# default language `ua`, default currency UAH) — PLAN.md M4.
seed-ua:
    cd backend && SEED_LOCALE=ua cargo run -p seeder

# Build the api docker image. Frontend is static — deployed to Vercel, not
# a docker image (see docs/adr/0010-frontend-hosting-vercel-instead-of-fly.md).
build:
    {{container_runtime}} build -f deploy/Dockerfile.api -t polymix-api backend
