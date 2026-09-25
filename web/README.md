# SorobanPulse Web Dashboard

A Vite + React 18 + TypeScript dashboard for exploring indexed Soroban events, managing subscriptions and monitoring the indexer.

## Prerequisites

- **Node.js ≥ 20** (LTS recommended)
- **npm ≥ 10**
- A running SorobanPulse API server (default: `http://localhost:3000`)

## Quick start

```bash
cd web
npm ci          # install exact locked versions
npm run dev     # start the dev server on http://localhost:5173
```

Open `http://localhost:5173` — the Vite dev proxy forwards `/api/*` to `http://localhost:3000` so you won't hit CORS issues.

Alternatively, from the repo root:

```bash
make web-dev
```

## Environment variables

Copy `.env.example` to `.env.local` and customise:

```
VITE_API_BASE_URL=http://localhost:3000   # API base URL (no trailing slash)
VITE_API_KEY=                             # Optional X-Api-Key header value
```

`.env.local` is git-ignored — never commit real secrets.

## Available scripts

| Command | Description |
|---|---|
| `npm run dev` | Start Vite dev server with HMR |
| `npm run build` | Type-check and build for production into `dist/` |
| `npm run preview` | Preview the production build locally |
| `npm run lint` | ESLint — fails on any warning or error |
| `npm run lint:fix` | ESLint with auto-fix |
| `npm run format` | Prettier write |
| `npm run format:check` | Prettier check (used in CI) |
| `npm run test` | Vitest (single run, no watch) |
| `npm run test:watch` | Vitest in watch mode |
| `npm run typecheck` | TypeScript compile check (no emit) |
| `npm run gen:api` | Regenerate `src/api/schema.d.ts` from `../openapi.json` |
| `npm run check:api` | Fail if `schema.d.ts` is out of date |

## Project structure

```
web/
├── src/
│   ├── api/           # Typed openapi-fetch client + TanStack Query hooks
│   ├── components/
│   │   └── layout/    # Shell layout: Sidebar, TopBar, Layout
│   ├── pages/         # Route-level page components
│   ├── test/          # Vitest setup and unit tests
│   ├── App.tsx        # React Router root
│   ├── index.css      # Global CSS variables and utility classes
│   └── main.tsx       # Application entry point
├── scripts/
│   └── check-api-up-to-date.mjs
├── .env.example
├── eslint.config.js
├── .prettierrc
├── tsconfig.json      # IDE / typecheck config
├── tsconfig.build.json
├── tsconfig.node.json
└── vite.config.ts     # Vite + Vitest config
```

## Tech stack

| Layer | Library |
|---|---|
| Build | [Vite 5](https://vitejs.dev) |
| UI | [React 18](https://react.dev) |
| Routing | [React Router 6](https://reactrouter.com) |
| Data fetching | [TanStack Query 5](https://tanstack.com/query) |
| API client | [openapi-fetch](https://openapi-ts.dev/openapi-fetch/) + generated types |
| Testing | [Vitest](https://vitest.dev) + [React Testing Library](https://testing-library.com) |
| Linting | ESLint 9 (flat config) + `@typescript-eslint` |
| Formatting | Prettier 3 |
