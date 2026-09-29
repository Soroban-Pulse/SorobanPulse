# Soroban Pulse Explorer

Browse, test, and inspect [Soroban Pulse](https://github.com/soroban-pulse/soroban-pulse) API endpoints directly from VS Code.

## Features

- **API Explorer** — sidebar tree of all endpoints grouped by category (Events, Contracts, Subscriptions, Admin, …)
- **Request Tester** — send real HTTP requests with path params, query params, custom headers, and a body editor
- **Response Viewer** — formatted JSON body, status badge, duration, and response headers
- **Contract hover** - hover a `C…` contract ID in Rust, TypeScript, JavaScript or JSON to see its total events, last seen ledger and last event time

## Getting Started

1. Install the extension.
2. Open **Settings** (`Ctrl+,`) and search for `sorobanpulse`:
   - Set `sorobanpulse.baseUrl` to your running instance (default: `http://localhost:3000`)
   - Run **Soroban Pulse: Set API Key** (and optionally **Set Admin API Key** for `/admin/*`) from the Command Palette. Keys are kept in VS Code's SecretStorage, never in `settings.json`; any key found in the old `sorobanpulse.apiKey` / `sorobanpulse.adminApiKey` settings is moved there automatically on activation.
3. Click the **⚡** icon in the activity bar to open the API Explorer.
4. Click any endpoint to open it in the Request Tester — fill in parameters and hit **Send**.

## Commands

| Command | Description |
|---------|-------------|
| `Soroban Pulse: Open Settings` | Jump to extension settings |
| `Soroban Pulse: Tail Contract Events` | Stream a contract's events live into the **Soroban Pulse Events** output channel |
| `Soroban Pulse: Stop Tailing Events` | Close the running event stream |
| Refresh (toolbar) | Reload the endpoint list |
| Copy URL (right-click) | Copy the full endpoint URL to clipboard |

## Publishing

```bash
cd vscode-extension
npm install
npm run package        # builds soroban-pulse-explorer-x.x.x.vsix
npm run publish        # publishes to VS Code Marketplace (requires vsce login)
```

Releases are automated: bump `version` in `package.json`, add the entry to
`CHANGELOG.md`, then push a matching `vscode-extension-vX.Y.Z` tag. The
`VS Code Extension Release` workflow packages the `.vsix` and publishes it to
the VS Code Marketplace (`VSCE_PAT` secret) and Open VSX (`OVSX_PAT` secret).

## Requirements

- VS Code `^1.85.0`
- A running Soroban Pulse server
