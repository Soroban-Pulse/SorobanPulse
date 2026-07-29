# Developer Onboarding (Issue #826)

A "start here" guide that ties together the setup instructions in the
[README](../README.md), the workflow in [CONTRIBUTING.md](../CONTRIBUTING.md), and
the [Architecture Guide](architecture.md) into one path for a new contributor's
first day.

## Before you start

- **Rust** (stable toolchain — this project does not pin a specific version via
  `rust-toolchain.toml`, so `rustup update stable` before starting is a good idea)
- **PostgreSQL 14+** (or Docker, to run one via `docker-compose.yml`)
- **Docker** (optional but recommended — `make docker-up` gets the full stack
  running without installing Postgres locally)

## Day-1 checklist

1. **Clone and configure.**
   ```bash
   git clone <repo-url> && cd SorobanPulse
   cp .env.example .env   # fill in real values — see README § Setup
   ```
2. **Get a database running before your first build.** This is the step most
   likely to trip up a first build — see [Why does `cargo build` need a
   database?](#why-does-cargo-build-need-a-database) below.
   ```bash
   docker compose up -d postgres   # or point DATABASE_URL at an existing instance
   export DATABASE_URL=postgres://postgres:postgres@localhost:5432/soroban_pulse
   ```
3. **Build.**
   ```bash
   cargo build
   ```
4. **Run the full test suite** (spins up its own throwaway database — see
   [CONTRIBUTING.md § Running Integration Tests](../CONTRIBUTING.md#running-integration-tests)):
   ```bash
   make test-db
   ```
5. **Run the server locally:**
   ```bash
   make run          # local binary, migrations run automatically on startup
   # or
   make docker-up    # full stack via Docker Compose
   ```
6. **Confirm it's alive:**
   ```bash
   curl http://localhost:3000/healthz/live
   curl http://localhost:3000/v1/events
   ```
7. **Install the pre-commit hooks** so `cargo fmt`/`clippy`/`check` run locally
   before every commit, catching most CI failures before you push — see
   [CONTRIBUTING.md § Pre-commit Hooks](../CONTRIBUTING.md#pre-commit-hooks).

If everything above works, you're set up. If something fails, check
[Common First-Build Issues](#common-first-build-issues) below before
[docs/troubleshooting.md](troubleshooting.md), which covers *running* issues
(indexer lag, rate limiting, SSE) rather than *build* issues.

## Why does `cargo build` need a database?

Most queries in this codebase use `sqlx::query_as` with a plain SQL string,
checked only at runtime — no database is needed to compile those. A handful
of call sites (e.g. `src/abi.rs`) use the compile-time–checked `sqlx::query!`
macro instead, which connects to `DATABASE_URL` **while compiling** to verify
the query against the live schema. If that database isn't reachable and
already migrated, `cargo build`/`cargo check` fails before you even get to
running anything.

If you hit an error mentioning `sqlx` and a connection failure or `SQLX_OFFLINE`
during a **build** (not a test run), it means: bring up Postgres and run
migrations first, *then* build.

```bash
docker compose up -d postgres
export DATABASE_URL=postgres://postgres:postgres@localhost:5432/soroban_pulse
cargo sqlx migrate run --source migrations   # or: cargo run (runs migrations on startup)
cargo build
```

## Common First-Build Issues

### `cargo build` fails immediately with a dependency resolution error

```
error: failed to select a version for the requirement `<crate> = "..."`
candidate versions found which didn't match: ...
```

This means a dependency pin in `Cargo.toml` no longer resolves against the
current crates.io index (the requested version was never published, or has
since been fully removed). This is a manifest problem, not something wrong
with your environment — `cargo update -p <crate> --precise <version>` will
not help if no compatible version exists. Check the project's issue tracker
for an open report before filing a new one, since a stale/incorrect version
pin affects every contributor identically until it's corrected in
`Cargo.toml`/`Cargo.lock`.

### `cargo build` fails with unresolved `crate::` paths in a binary target

If `cargo build` (not `cargo build --lib`) fails with `unresolved module` or
`cannot find X in this scope` errors pointing at `src/main.rs`, a module that
exists under `src/` and is wired into the library target (`src/lib.rs`) was
not also declared with `mod ...;` in `src/main.rs` — the binary and library
targets each maintain their own module tree from the same `src/` files (see
`src/main.rs` vs. `src/lib.rs`), so a new module has to be added to *both*
when it's needed from handler code. If you hit this on a module we haven't
covered yet, adding the missing `mod <name>;` line to `src/main.rs` is
usually the entire fix.

### `docker-compose up` fails with a port conflict

Postgres (`5432`) or the app port (`3000`) is already bound by another local
process or a previous Compose run that wasn't torn down cleanly.

```bash
docker compose down
lsof -i :5432   # or :3000 — find what's holding the port
```

### `make test-db` hangs or fails to connect

The throwaway Postgres container may not be healthy yet, or Docker itself
isn't running. Check `docker compose -f docker-compose.test.yml ps` and
`docker compose -f docker-compose.test.yml logs postgres`.

### Migrations fail with a permissions error

See [docs/troubleshooting.md § Migrations fail on startup](troubleshooting.md#migrations-fail-on-startup)
for the exact grant statement needed.

## Where to go next

- **Understand the system**: [Architecture Guide](architecture.md) — component
  descriptions, event flow, the multi-replica advisory lock mechanism, and
  deployment architecture.
- **Understand the schema**: [Database Schema](schema.md) — table structure,
  indexes, and an ER diagram.
- **Understand testing conventions**: [CONTRIBUTING.md](../CONTRIBUTING.md) —
  commit message format, migration naming, fuzzing, mutation testing, and the
  PR checklist.
- **Understand performance work**: [Performance Tuning Guide](performance-tuning.md)
  and [Database Configuration Tuning](database-configuration-tuning.md).
- **Pick a first issue**: look for issues labeled `good first issue` (if none
  are labeled yet, start with something in `docs/` — documentation fixes are
  a low-risk way to learn the codebase layout before touching `src/`).

## Feedback

Found a step here that's wrong, missing, or took longer than expected? Open a
PR against this file directly, or open an issue describing what tripped you
up — a "this confused me" report from someone on their first day is exactly
the signal this document needs to stay accurate. See
[CONTRIBUTING.md § Pull Requests](../CONTRIBUTING.md#pull-requests) for the
process.
