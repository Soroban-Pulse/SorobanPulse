## Summary

First slice of the batch-2 CLI items #1133-#1136: `spulse tail` streams live events as NDJSON, `spulse completions` generates shell completions (and the Homebrew formula now installs them), `spulse --version` shows the git commit, and the binary gets its first integration tests against a mocked API. Each issue is only partly implemented; the lists below give the exact scope.

## Related Issue

Closes #1133, #1134, #1135, #1136 (see the trailing block).

## Changes

### #1133 `spulse tail`: live event streaming in the terminal

What existed: query / export / subscription commands only. Every request goes through one blocking `reqwest` client with `timeout(timeout_secs)`, which would cut a live stream off.

Done:
- `spulse tail --contract <id> [--type <event_type>]` opens `/v1/events/stream?contract_id=&event_type=` with `Accept: text/event-stream` and the API key
- `ApiClient::stream` uses a separate client with no overall timeout and reports non-2xx as `HTTP <status>: <body>`
- `src/tail.rs`: SSE parser (multi-line `data`, keep-alive comments, `event`, `id`/`last_event_id`, CRLF) and `pipe_ndjson`, which writes each event as one compact JSON line and flushes it; non-JSON frames are skipped; a closed stdout pipe ends quietly
- Checked live against a local SSE server sending an event every 0.7 s: `spulse tail -c ... | jq .ledger` prints each ledger as it arrives
- README section (including `jq --unbuffered` when jq's own output is piped)

Not done in this PR:
- `--format table|json|ndjson` and coloured compact output (always NDJSON for now)
- Reconnect and resume with `Last-Event-ID` (the parser already tracks the last id)
- `--since-ledger` replay
- Ctrl-C summary line (events received, duration)

### #1134 Shell completions and man page generation

What existed: `cli/homebrew/spulse.rb` ran `spulse --generate <shell>` in `post_install`, a flag that does not exist, so it wrote empty completion files (errors were swallowed by `|| true`).

Done:
- `spulse completions <bash|zsh|fish|powershell|elvish>` via `clap_complete`; handled before config loading so a broken config file cannot break it
- Formula uses Homebrew's `generate_completions_from_executable(bin/"spulse", "completions")` in `install` and tests `spulse completions zsh`
- Verified: all four shells generate; in bash, `spulse ta<Tab>` completes to `tail`

Not done in this PR:
- Man page via `clap_mangen`
- Installation docs in `cli/README.md`

### #1135 Prebuilt release binaries and automated Homebrew updates

Done:
- `cli/build.rs` embeds the commit: `spulse --version` prints `spulse 0.1.0 (<10-char sha>)`. `SPULSE_GIT_SHA` overrides it (tarball / CI builds); falls back to `unknown` without git. Rebuilds when HEAD or the branch ref moves

Not done in this PR:
- `cargo-dist` configuration for linux (x86_64/aarch64, musl), macOS and windows
- `cli-v*` release workflow with archives, checksums and shell/powershell installers
- Automatic formula publishing to a tap repository

### #1136 Integration tests with `assert_cmd` and a mocked API

What existed: 6 unit tests in `src/`, no tests of the binary. `mockito` (synchronous, fits the blocking client) was already a dev-dependency.

Done:
- `cli/tests/cli.rs` (11 tests, `assert_cmd` + `predicates` + `tempfile`): each test runs the real binary with `SPULSE_BASE_URL` pointed at a `mockito` server, config isolated through `HOME` / `XDG_CONFIG_HOME`, `NO_COLOR=1`
- `events`: JSON output with query and `x-api-key` matched, contract endpoint with CSV output, HTTP 500 with the server message
- `stats`: table output, HTTP 401
- `tail`: NDJSON parsed line by line from a mocked stream, HTTP 400, missing `--contract`
- `config path`, `completions` (valid and invalid shell), `--version` format

Not done in this PR:
- Tests for `contracts`, `export`, `subscriptions`, `webhook-test` and the other `config` actions
- `insta` snapshot tests for table and CSV output
- Config precedence tests (flag > env > file)
- CI job for the CLI crate

## Testing

- [x] `cargo test` passes (in `cli/`: 10 unit + 11 integration)
- [ ] `cargo clippy` reports no warnings (`cargo clippy --all-targets` shows 7 warnings, all on lines already on `main` in `exporter.rs`, `client.rs`, `query.rs`, `formatter.rs`, `webhook_test.rs`, `main.rs`; this PR adds none)
- [x] Manually tested locally

## Notes

- New files are `rustfmt`-clean; existing CLI files use hand-aligned formatting and were not reformatted. `cli/` is not a member of the root workspace, so root CI does not build or test it.
- Found while testing, not changed here: `spulse stats --contract <id>` decodes `/v1/contracts/{id}/summary` as `EventStats`, but that endpoint returns `ContractDetailSummary` (no `total_contracts`), so it fails against a real server.

Closes #1133
Closes #1134
Closes #1135
Closes #1136
