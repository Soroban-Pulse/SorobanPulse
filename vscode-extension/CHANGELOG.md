# Changelog

All notable changes to the Soroban Pulse Explorer extension are documented
here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)
and the extension uses [Semantic Versioning](https://semver.org/).

Releases are cut by pushing a `vscode-extension-vX.Y.Z` tag that matches
`version` in `package.json`; see `.github/workflows/vscode-extension-release.yml`.

## [Unreleased]

### Added

- Hover a `C…` contract ID in Rust, TypeScript, JavaScript or JSON files to
  see its total events, last seen ledger and last event time (#1125).
- Tag-driven release workflow publishing to the VS Code Marketplace and
  Open VSX (#1126).

## [0.1.0]

### Added

- API Explorer sidebar listing all endpoints by category.
- Request Tester with path/query params, headers and body editor, and a
  Response Viewer.
- `Set API Key`, `Set Admin API Key` and `Clear Stored API Keys` commands
  backed by VS Code SecretStorage (#963).
- `Test Webhook` command (#963).
