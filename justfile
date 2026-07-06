# Falls back to podman when docker isn't on PATH (e.g. local dev on this
# machine) — CI runners have docker, so this is a no-op there.
container_runtime := `command -v docker >/dev/null 2>&1 && echo docker || echo podman`

# Bring up SurrealDB + the API (dev auth mode) + the Vite dev server.
dev:
    {{container_runtime}} compose -f deploy/compose.yaml up -d --wait
    trap 'kill 0' EXIT INT TERM; \
    (cd backend && AUTH_DEV_MODE=true PORT=8080 cargo run -p api) & \
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

# Backend integration tests (testcontainers).
test-int:
    cd backend && cargo test --workspace -- --ignored

# Seeder against the local dev tenant (50k customers, 200k orders).
seed:
    cd backend && cargo run -p seeder

# Build all docker images.
build:
    {{container_runtime}} build -f deploy/Dockerfile.api -t polymix-api backend
    {{container_runtime}} build -f deploy/Dockerfile.frontend -t polymix-frontend frontend
