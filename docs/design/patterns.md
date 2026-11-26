# Dashboard State Patterns

Every data-backed view in the dashboard has three non-happy states. Use these shared
components instead of ad-hoc markup. Implementation details for the skeleton primitives
are in [`dashboard/STATE_PATTERNS.md`](../../dashboard/STATE_PATTERNS.md); visual references
are in [`event-explorer/`](event-explorer/). All three are in Storybook under
**Design System / State Patterns**.

## `LoadingState`

Skeleton that mimics the layout about to render, so the page doesn't jump.

| Prop | Type | Default | Notes |
|------|------|---------|-------|
| `layout` | `"table" \| "cards" \| "chart"` | `"table"` | Match the content that will replace it |
| `rows` | `number` | `5` | Table rows, or number of cards |
| `columns` | `number` | `4` | Table columns |
| `label` | `string` | `"Loading…"` | Screen-reader announcement |

- Use for the **first** load of a region. For refreshes of already-shown data, keep the old data and show a subtle indicator.
- Match `rows` to the page size so pagination doesn't shift.
- Never use a bare spinner or "Loading…" text for tables.

## `EmptyState`

| Prop | Type | Notes |
|------|------|-------|
| `icon` | `ReactNode` | Optional; defaults to `IconEmpty` |
| `title` | `string` | Says what's missing: "No events match your filters" |
| `description` | `string` | Says what to do next |
| `actionLabel` / `onAction` | `string` / `() => void` | Optional CTA; both required to render the button |

- Distinguish *nothing exists yet* (`EmptyEvents`, onboarding CTA) from *filters exclude everything* (`EmptyFilterResults`, "Clear filters" CTA).

## `ErrorBanner`

| Prop | Type | Notes |
|------|------|-------|
| `title` | `string` | What failed: "Couldn't load events." |
| `message` | `string` | Optional detail / status code |
| `onRetry` | `() => void` | Shows a Retry button |
| `retryLabel` | `string` | Defaults to "Retry" |
| `onDismiss` | `() => void` | Shows a dismiss button |

- Inline above the affected region, `role="alert"`. Keep filters and surrounding UI usable.
- Use full-page `ErrorState` variants (`ErrorUnauthorized`, `ErrorServerUnreachable`, `ErrorRateLimited`, `Error5xx`) only when the whole page is unusable.
- Don't show an empty state at the same time as an error.

## Applied in

- **Event Explorer** (`EventExplorerPage.tsx`): `LoadingState` table skeleton while fetching, `ErrorBanner` with retry on failure, `EmptyFilterResults` when no rows match.
- **Contract Overview**: no dedicated page exists in the dashboard yet; it must adopt these patterns when built.
