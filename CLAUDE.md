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
