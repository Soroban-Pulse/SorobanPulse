# Contributing to Soroban Pulse

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

## Code Style

- Formatting is enforced by `rustfmt` using the project's `rustfmt.toml` (`max_width = 100`, `edition = "2021"`).
- Run `make fmt` before pushing, or let the pre-commit hook handle it.
- CI will reject any PR where `cargo fmt --check` fails.

## Pull Requests

- Keep PRs focused — one logical change per PR.
- Ensure `make test` and `make lint` pass locally before opening a PR.
- Write a clear PR description referencing the relevant issue (e.g. `Closes #75`).
