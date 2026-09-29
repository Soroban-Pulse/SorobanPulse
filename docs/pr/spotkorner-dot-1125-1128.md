## Summary

First slice of the batch-2 backlog items #1125-#1128: a contract ID hover in the VS Code extension, an extension changelog with a tag-driven Marketplace / Open VSX release, a buildable and packable `@soroban-pulse/sdk` TypeScript package, and a written SDK versioning policy. Each issue is only partly done; the lists below give the exact scope.

## Related Issue

Part of #1125, #1126, #1127, #1128 (see the trailing block).

## Changes

### #1125 Hover provider for contract IDs in source files

What existed: nothing; `GET /v1/contracts/{contract_id}/summary` returns `total_events`, `last_event_at` and `ledger_range`.

Done:
- `HoverProvider` for Rust, TypeScript(React), JavaScript(React), JSON and JSONC matching `C[A-Z2-7]{55}`; shows total events, last seen ledger (`ledger_range.max`) and last event time
- 30 s cache (concurrent hovers share one request, failures are cached too, cache cleared when a `sorobanpulse.*` setting changes)
- Quiet errors: unknown contract, non-200, invalid JSON, 3 s timeout or unreachable server -> no hover
- `onLanguage:*` activation events so hovers work before the explorer view is opened
- Core in `src/contractHover.ts` (no `vscode` import) with `node:test` tests; `npm test` script

Not done in this PR:
- Contract label
- "Open in Explorer" / "Tail events" links
- Setting to disable the hover

### #1126 Extension tests and Marketplace / Open VSX publish workflow

Done:
- `.github/workflows/vscode-extension-release.yml` on `vscode-extension-v*` tags: checks the tag matches `package.json` `version`, `npm ci`, `vsce package`, uploads the `.vsix`, publishes with `VSCE_PAT` (Marketplace) and `OVSX_PAT` (Open VSX) secrets
- `vscode-extension/CHANGELOG.md` (Keep a Changelog) and release steps in the extension README

Not done in this PR:
- `@vscode/test-electron` integration tests (activation, tree view, commands) and running them under xvfb in CI
- Unit tests for request building / response rendering (that code currently lives inside the webview script)
- Note: `vsce package` still needs `media/icon.png` (tracked in #1122) before the workflow can publish

### #1127 Make the TypeScript SDK a publishable npm package

What existed: sources and `__tests__/sse.test.ts`, no `package.json` / `tsconfig.json`. The sources did not compile on `main`:
- `apis/EventsApi.ts` was missing the class's closing `}` (merge `6a8a303`)
- the hand-added SSE methods used `this.basePath`, which `BaseAPI` does not have
- `interceptors.ts` read the non-standard `Response.request`
- `sse.ts` assigned `string | undefined` to a `Required<>` field, and `isConnected()` always returned `true` (`(this as any).fetchController !== null` with the field `undefined`), which failed 3 of the 17 SSE tests

Done:
- `package.json` for `@soroban-pulse/sdk`: tsup dual build (`dist/index.js` CJS, `dist/index.mjs` ESM, `.d.ts` / `.d.mts`), `exports` map, `types`, `files`, `prepack`; `package-lock.json`
- Strict `tsconfig.json` (`npm run typecheck`), `jest.config.cjs` (ts-jest) so `__tests__/sse.test.ts` runs
- Minimal source fixes for the problems above: closing brace, `this.configuration.basePath`, drop `Response.request` (same caching behaviour), `apiKey ?? ''`, a typed `fetchController` field (fixes `isConnected()`)
- `.openapi-generator-ignore` lists the hand-written files and package tooling so `make generate-sdk` leaves them alone
- `examples.ts` is excluded from the typecheck: it calls methods that no longer exist (`getHealthz`, `getEventsExport`, ...); it is not part of the package

Not done in this PR:
- CI job (install, build, test, `npm pack --dry-run`)
- `sdk/typescript/README.md` install instructions
- Note: `EventsApi.ts` is both generated and hand-edited (SSE methods); regenerating it would drop those methods. Not addressed here

### #1128 Automated release workflows for npm, PyPI and Go module tags

Done:
- "SDK Versioning" section in `docs/api-versioning.md`: SemVer, SDK major tracks API major, all SDKs released together at one version, `sdk-vX.Y.Z` release tags plus `sdk/go/vX.Y.Z` Go tags, package names, support for deprecated API versions
- Records two release prerequisites: the Python PyPI name (`openapi_client` today) and the Go module path (`github.com/soroban-pulse/client-go` must become `github.com/Soroban-Pulse/SorobanPulse/sdk/go` for subdirectory tags to resolve)

Not done in this PR:
- `sdk-release.yml` (npm with provenance, PyPI trusted publishing, Go tag)
- Per-SDK changelogs
- Install badges in the SDK READMEs

## Testing

- [ ] `cargo test` passes (N/A: no Rust changes)
- [ ] `cargo clippy` reports no warnings (N/A: no Rust changes)
- [x] Manually tested locally

Verification:
- `vscode-extension`: `npm ci && npm test` compiles and passes 5 hover tests. The provider was smoke-tested under Node with a stubbed `vscode` module against a local server: one request for repeat hovers, `x-api-key` sent, no hover for unknown IDs, text without an ID, or a stopped server
- `sdk/typescript`: `npm run typecheck` clean, `npm run build` emits CJS + ESM + types, `npm test` 17/17 (14/17 before the `isConnected` fix). `npm pack` tarball installed into an empty project imports from both `require()` and `import`
- Release workflow YAML parses; not run (needs tags and secrets)

## Notes

#1125 and #1126 touch `vscode-extension/` alongside #1199 (`extension.ts`, `package.json`, the same `npm test` script line); overlaps are small and textual.

Part of #1125
Part of #1126
Part of #1127
Part of #1128
