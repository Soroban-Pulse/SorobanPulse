## Summary

First slice of the batch-2 backlog items #1121-#1124: a README header banner, a lint-clean VS Code extension with correct repository metadata, automatic migration of plaintext API keys into `SecretStorage`, and a "Tail Contract Events" command. Each issue is only partly done; the lists below give the exact scope so the rest can be picked up separately.

## Related Issue

Part of #1121, #1122, #1123, #1124 (see the trailing block).

## Changes

### #1121 GitHub repository presentation assets

What existed: no `docs/assets/`, no README banner.

Done:
- `docs/assets/banner-light.svg` and `docs/assets/banner-dark.svg` (1280x320, same layout, palette matches `docs/architecture.svg`)
- Root `README.md` header uses them through `<picture>` with a `prefers-color-scheme: dark` source
- `docs/assets/README.md` lists the assets and where they are used

Not done in this PR:
- 1280x640 social preview image and the repo-settings change to use it (needs admin)
- Annotated dashboard screenshots / GIF
- Badges and the "what is SorobanPulse" showcase graphic

### #1122 Make the VS Code extension buildable and packageable in CI

What existed: `package-lock.json` already committed and `npm run compile` passed, but `npm run lint` failed because there was no ESLint config at all; `repository.url` pointed at `soroban-pulse/soroban-pulse`.

Done:
- `vscode-extension/.eslintrc.json` (`eslint:recommended` + `@typescript-eslint/recommended`); fixed the one finding (unused `ApiEndpoint` import in `apiData.ts`). `npm ci && npm run compile && npm run lint` now pass
- `repository.url` -> `https://github.com/Soroban-Pulse/SorobanPulse.git` with `directory: vscode-extension`; added `bugs` and `homepage`

Not done in this PR:
- `media/icon.png` (icon or placeholder), so `vsce package` still fails on the missing icon
- The CI job (`vscode-extension/**` paths filter, compile + lint + `vsce package`, `.vsix` artifact upload)

### #1123 Store API keys in VS Code `SecretStorage`

What existed (from #963): "Set API Key" / "Set Admin API Key" / "Clear Stored API Keys" commands backed by `context.secrets`, and `requestTester.ts` already reads keys from secrets first. The plaintext settings were still honoured and never cleaned up.

Done:
- `src/keyMigration.ts`: on activation, each non-empty `sorobanpulse.apiKey` / `sorobanpulse.adminApiKey` is copied into `SecretStorage` (an already-stored secret wins) and removed from every scope that set it (user and workspace; the settings are window-scoped), with one notification naming the moved settings
- The module has no `vscode` import; `apiKeyManager.ts` adapts `WorkspaceConfiguration.inspect/update`
- Setting descriptions, extension README and `docs/vscode-extension.md` now point to the command instead of the setting
- `npm test` script (compile + `node --test out/test/*.test.js`); `out/test/**` was already in `.vscodeignore`

Not done in this PR:
- Separate keys per configured server URL

### #1124 Live event tail panel

What existed: server-side `/v1/events/stream?contract_id=` (unnamed SSE `message` frames, JSON `data`, `id`, `Last-Event-ID` replay); nothing in the extension.

Done:
- "Soroban Pulse: Tail Contract Events": prompts for a contract ID (pre-filled from a `C…` strkey in the editor selection, validated), streams the endpoint with the stored `x-api-key`, and writes each event to the **Soroban Pulse Events** output channel as a summary line (type, ledger, tx) plus pretty JSON. Non-200 responses and connection errors are reported in the channel
- "Soroban Pulse: Stop Tailing Events"; one tail at a time, disposed with the extension
- `src/eventStream.ts`: incremental SSE parser (chunk-split lines, CRLF split across chunks, comments, multi-line data, named events, tracks `lastEventId`), contract-ID detection, formatting

Not done in this PR:
- Webview with pause / clear / filter-by-type controls
- Status bar item showing connection state
- Reconnect with `Last-Event-ID` (the parser already tracks the last id for this)

## Testing

- [ ] `cargo test` passes (N/A: no Rust changes)
- [ ] `cargo clippy` reports no warnings (N/A: no Rust changes)
- [x] Manually tested locally

Verification, in `vscode-extension/`:
- `npm ci && npm run compile && npm run lint` pass (lint failed on `main` with no config)
- `npm test`: 11 tests pass (4 key migration, 7 SSE parser / contract ID / formatting)
- `EventTail` smoke-tested under Node with a stubbed `vscode` module against a local SSE server: request hits `/v1/events/stream?contract_id=<selection>` with `x-api-key`, both events print, server close is reported
- Banners rendered in headless Chrome in both variants

## Notes

No Rust, API, schema or deployment changes, so no ADR.

Part of #1121
Part of #1122
Part of #1123
Part of #1124
