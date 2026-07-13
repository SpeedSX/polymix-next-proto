# Agent notes: environment setup

## Spinning up the dev stack

`just dev` is the human/interactive entry point — it foregrounds `cargo run
-p api` and `npm run dev` behind a `wait` and never returns. **Don't run it
from an agent shell call** — it hangs the tool call, or leaves the dev
servers attached to a backgrounded job that dies the moment that job stops.

To just get **SurrealDB** up (before `just seed`, before starting the API
manually, or before `scripts/perf-search.sh`), bring up `deploy/compose.yaml`
directly — detached, healthchecked, returns immediately:

```
<runtime> compose -f deploy/compose.yaml up -d --wait
```

where `<runtime>` is whichever of `docker`/`podman` is on PATH (check both;
either may be present, e.g. `podman machine list` / `podman ps` for a
podman VM). The container keeps running after the command returns and after
the session ends — tear it down with `<runtime> compose -f
deploy/compose.yaml down` when done, or leave it for next time.

If the API itself needs to run too, start it as its own background step
rather than through `just dev`, e.g. via the Bash tool's
`run_in_background`: `cd backend && AUTH_DEV_MODE=true PORT=8080 cargo run
-p api`.

`just seed` loads the 50k-customer/200k-order demo tenant used for perf
testing (`docs/perf.md`, `scripts/perf-search.sh`) — it connects to
SurrealDB directly (`Store::connect`), so it only needs the DB up, not the
API.

To measure `/api/search` latency end to end, use the `perf-check` skill
(`.claude/skills/perf-check/SKILL.md`) instead of re-deriving these steps —
it also covers the gotcha: never background `cargo run -p api` with a
shell-level `(cmd &)` fork on this Windows + git-bash setup, it orphans the
process outside the harness's tracking.

## Container runtime: docker or podman, don't assume either

The container runtime on PATH varies by machine — `docker`, `podman`, or
both. Check before assuming one is missing.

- `justfile`'s `container_runtime` variable auto-detects this already, so
  `just dev`, `just test-int`, and `just build` work unmodified.
- The Rust integration-test harness (`testcontainers` crate, used by
  `crates/api/tests/common/mod.rs`) does **not** go through the justfile —
  it talks to the Docker Engine API directly (via `bollard`) and defaults
  to the named pipe `npipe:////./pipe/docker_engine` on Windows. If only
  podman is available, point it at podman's Docker-API-compatible pipe:

  ```
  export DOCKER_HOST="npipe:////./pipe/podman-machine-default"
  ```

  Confirm the exact pipe name with `podman machine inspect`
  (`ConnectionInfo.PodmanPipe.Path`) if it's not the default machine name.

## Deploying: Fly.io (api) + SurrealDB Cloud (db) + Vercel (frontend)

Two deviations from PLAN.md's original M6 design (three Fly apps): the
frontend is on Vercel, not a Fly nginx app (`docs/adr/0010`), and SurrealDB
runs on its Cloud free tier, not a self-hosted Fly app (`docs/adr/0011`).
Only the api is still a Fly app.

### Fly: api

One Fly app, `backend/fly.toml`. Configured for minimum cost as a
prototype: `shared-cpu-1x`/256mb, `auto_stop_machines` down to zero when
idle — the api's own `Store::connect` retry (30s deadline) covers cold
starts against SurrealDB Cloud same as it would against a Fly-hosted DB.

Ships with `AUTH_DEV_MODE=true` (no real Clerk setup yet) — fine for a
private/demo-only prototype, **not** for a shared link or real data: dev
mode leaves `/dev/token` live on a public URL and CORS permissive. Switching
to Clerk is a follow-up: flip `AUTH_DEV_MODE=false` and add
`AUTH_ISSUER`/`AUTH_JWKS_URL`/`CORS_ALLOWED_ORIGINS` (the latter must list
the Vercel domain, e.g. `https://polymix.vercel.app`, not a `.fly.dev` one)
on `polymix-api`, then redeploy the frontend with `VITE_AUTH_MODE=clerk` +
`VITE_CLERK_PUBLISHABLE_KEY` set as Vercel env vars.

One-time setup:

```
fly apps create polymix-api
fly secrets set -a polymix-api SURREALDB_USER=<root user from SurrealDB Cloud> SURREALDB_PASS=<its password>
fly deploy --config backend/fly.toml backend
```

Redeploy after code changes with the matching `fly deploy` line above — no
need to repeat `fly apps create`/`fly secrets set` unless the app or secret
values change.

### SurrealDB Cloud: db

Free tier (`docs/adr/0011`) — 1GB storage, root-auth-compatible with our
existing `Store::connect`, no code changes, just config. Create the
instance yourself via the [SurrealDB Cloud dashboard](https://surrealdb.com/cloud)
(free tier, latest v3.x version, any region — closer to `polymix-api`'s
`primary_region` is marginally better but not required for a prototype),
then:

1. Copy the instance's `wss://` endpoint from its "Connect" tab into
   `backend/fly.toml`'s `SURREALDB_URL` (replacing the `CHANGE-ME`
   placeholder).
2. Set its root username/password as `polymix-api`'s Fly secrets (the
   `fly secrets set` line above).
3. `fly deploy --config backend/fly.toml backend` to pick up the new URL.

1GB comfortably fits the small `ua` demo tenant (`just seed-ua` — 100
customers/1,000 orders) but not the 50k/200k perf-seed tenant (`just
seed`) — that needs the self-hosted Fly fallback in
`deploy/fly.surrealdb.toml` (kept, not deleted — see its header comment)
when real M6 perf testing happens.

### Vercel: frontend

`frontend/vercel.json` has the SPA fallback rewrite (TanStack Router routes
are client-side; without it, refreshing on `/customers` 404s). Build
command/output directory are Vercel's zero-config Vite defaults — nothing
else to set. Run every command from inside `frontend/` (or `--cwd frontend`
from the repo root) so Vercel treats it as the project root, no dashboard
"Root Directory" setting needed:

```
cd frontend
vercel login                 # one-time, opens a browser
vercel link                  # one-time, creates/links the Vercel project
vercel env add VITE_API_URL production        # value: https://polymix-api.fly.dev
vercel env add VITE_WS_URL production         # value: wss://polymix-api.fly.dev
vercel env add VITE_AUTH_MODE production      # value: dev (or clerk once Clerk is wired)
vercel deploy --prod
```

Redeploy with `vercel deploy --prod` after frontend changes, or after
changing any `VITE_*` env var (Vite bakes them into the bundle at build
time — changing the value in the dashboard alone does nothing until the
next build).

## Running tests

- `cargo test --workspace` (from `backend/`) runs everything *except* the
  `#[ignore]`-gated integration tests — no container runtime needed.
- Integration tests (real SurrealDB via testcontainers — search ranking,
  tenant isolation, order/invoice flows) need `DOCKER_HOST` set as above if
  running podman, then either:
  - `just test-int` (from repo root — also cleans up containers after), or
  - `cargo test --workspace -- --ignored` (from `backend/`, manual cleanup:
    `<runtime> rm -f $(<runtime> ps -aq --filter "label=org.testcontainers.managed-by=testcontainers")`).
- See `README.md`'s "Integration tests" section for the human-facing version
  of this same note.
