# Contributing to Soroban Pulse

## Running Integration Tests

Integration tests require a live PostgreSQL instance. The easiest way is `make test-db`, which starts a throwaway container, runs the full suite, then tears it down:

```bash
make test-db   # only Docker required — no local Postgres needed
```

Under the hood this runs:

```bash
docker compose -f docker-compose.test.yml up -d --wait
DATABASE_URL=postgres://postgres:postgres@localhost/soroban_pulse_test cargo test
docker compose -f docker-compose.test.yml down
```

If you already have a Postgres instance running, set `DATABASE_URL` directly:

```bash
export DATABASE_URL=postgres://<user>:<password>@localhost/<dbname>
cargo test
```

`sqlx::test`-annotated tests create and tear down their own isolated database automatically — no manual schema setup needed.

## Getting Started

1. Fork the repo and create a branch from `main`.
2. Copy `.env.example` to `.env` and fill in your values. **Never commit `.env` or any `.env.*` file.**
3. Run `make docker-up` to start the full stack, or `make run` for the dev server only.

## Common Tasks

Run `make help` to see all available targets:

```
make build       # Compile the project
make test        # Run the full test suite (requires DATABASE_URL)
make lint        # Run clippy with warnings as errors
make fmt         # Format source code
make run         # Start the development server
make docker-up   # Start the full stack via Docker Compose
make docker-down # Tear down the Docker Compose stack
make migrate     # Run pending database migrations
make clean       # Remove build artifacts
```

## Pre-commit Hooks

This project uses [lefthook](https://github.com/evilmartians/lefthook) to run `cargo check`, `cargo fmt --check`, and `cargo clippy` before every commit.

Install lefthook and register the hooks once:

```bash
# macOS
brew install lefthook

# Linux / other (via cargo)
cargo install lefthook

# Register hooks in your local clone
lefthook install
```

Hooks typically complete in under 30 seconds on a typical change. If a hook fails, fix the reported issue and re-commit.

## Security: Never Log Sensitive Values

Never log passwords, API keys, tokens, or other credentials at any log level. This includes:

- `DATABASE_URL` — use `config.safe_db_url()` which strips credentials before logging
- `STELLAR_RPC_URL` — already sanitized by `validate_rpc_url()` before being stored in `Config`; the stored `config.stellar_rpc_url` is safe to log
- `API_KEY` / `API_KEY_SECONDARY` — never log these values
- Any request header that may contain `Authorization` or `X-Api-Key` values

When adding new log statements, double-check that no field contains a raw secret. If in doubt, strip credentials before logging (see `Config::safe_db_url()` for the pattern).

## Code Style

- Formatting is enforced by `rustfmt` using the project's `rustfmt.toml` (`max_width = 100`, `edition = "2021"`).
- Run `make fmt` before pushing, or let the pre-commit hook handle it.
- CI will reject any PR where `cargo fmt --check` fails.

## Pull Requests

- Keep PRs focused — one logical change per PR.
- Ensure `make test` and `make lint` pass locally before opening a PR.
- Write a clear PR description referencing the relevant issue (e.g. `Closes #75`).

## Releases

See [RELEASE.md](RELEASE.md) for the complete release process, including:
- Version numbering (Semantic Versioning)
- Updating CHANGELOG.md
- Creating git tags and GitHub releases
- Docker image publishing

Only maintainers can cut releases. If you'd like to propose a release, open an issue or contact the maintainers.
