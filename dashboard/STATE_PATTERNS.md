# SorobanPulse Dashboard — State Patterns

This document describes the design patterns for loading, empty, and error states used throughout the SorobanPulse dashboard. Each pattern includes a ready-to-implement React component and corresponding CSS.

---

## 1. Skeleton Loaders

Skeleton loaders provide a visual placeholder while content is being fetched. They use a shimmer animation to indicate that data is loading.

### Components

| Component | Use Case |
|-----------|----------|
| `SkeletonCard` | Placeholder for stat tiles and summary cards |
| `SkeletonTable` | Placeholder for data tables (configurable rows & columns) |
| `SkeletonChart` | Placeholder for chart areas (line charts, bar charts) |

### Usage

```tsx
import { SkeletonCard, SkeletonTable, SkeletonChart } from "./components";

// Card skeleton
<SkeletonCard width="12rem" height="80px" />

// Table skeleton (5 rows, 4 columns)
<SkeletonTable rows={5} columns={4} />

// Chart skeleton
<SkeletonChart height="300px" />
```

### Pattern

- Shimmer animation uses a linear gradient that sweeps left-to-right.
- Animation duration: 1.5s, infinite loop.
- Skeleton background: `#e8e8e8` → `#f5f5f5` → `#e8e8e8`.
- Chart skeletons show randomized bar heights to suggest data shape.
- Table header cells are slightly taller (1.25rem) than body cells (1rem).

---

## 2. Empty States

Empty states communicate that there is no data to display, with context-specific messaging and optional actions.

### Components

| Component | Scenario |
|-----------|----------|
| `EmptyEvents` | No events ingested yet — indexer is catching up |
| `EmptyFilterResults` | No events match the current filter criteria |
| `EmptySubscriptions` | User has no subscriptions configured |

### `EmptyEvents` — Indexer Catching Up

**When to show:** The events endpoint returns an empty list and the indexer is known to be syncing.

**Content:**
- Icon: Clock (⏱)
- Title: "No events yet"
- Description: Includes optional lag information (e.g., "The indexer is catching up — current lag is 12s.")

**Props:**
```tsx
interface EmptyEventsProps {
  lagSeconds?: number;  // If provided, shown in the description
}
```

### `EmptyFilterResults` — No Filter Matches

**When to show:** A user has applied filters (search, date range, event type) but no results match.

**Content:**
- Icon: Filter (🔍)
- Title: "No results"
- Description: Suggests adjusting the search or date range.
- Optional "Clear filter" action button.

**Props:**
```tsx
interface EmptyFilterResultsProps {
  onClearFilter?: () => void;
}
```

### `EmptySubscriptions` — No Subscriptions

**When to show:** The subscriptions list is empty for the authenticated user.

**Content:**
- Icon: Subscriptions (📋)
- Title: "No subscriptions"
- Description: Explains what subscriptions do and encourages creation.
- Optional "Create subscription" action button.

**Props:**
```tsx
interface EmptySubscriptionsProps {
  onCreateSubscription?: () => void;
}
```

### Generic `EmptyState`

All empty state components are built on the `EmptyState` primitive, which accepts:

```tsx
interface EmptyStateProps {
  icon?: React.ReactNode;        // Override the default icon
  title: string;                 // Heading text
  description: string;           // Body text
  actionLabel?: string;          // Optional button label
  onAction?: () => void;         // Optional button handler
}
```

---

## 3. Error States

Error states communicate what went wrong and offer recovery actions. They are designed to handle specific HTTP error scenarios.

### Components

| Component | HTTP Scenario |
|-----------|---------------|
| `ErrorUnauthorized` | 401 — user lacks permission or session expired |
| `ErrorServerUnreachable` | Network failure — server is down or unreachable |
| `ErrorRateLimited` | 429 — too many requests, shows `Retry-After` |
| `Error5xx` | 500/502/503/504 — server-side error with request ID |

### `ErrorUnauthorized`

**When to show:** The API returns 401 or the user's session is invalid.

**Content:**
- Icon: Lock (🔒)
- Title: "Unauthorized"
- Description: Instructs the user to sign in with an authorized account.
- Optional "Sign out and sign in again" action.

**Props:**
```tsx
interface ErrorUnauthorizedProps {
  onRetry?: () => void;
}
```

### `ErrorServerUnreachable`

**When to show:** `fetch` throws a network error (no response at all).

**Content:**
- Icon: Server (🖥)
- Title: "Server unreachable"
- Description: Suggests checking network connection.
- Optional "Retry" action.

**Props:**
```tsx
interface ErrorServerUnreachableProps {
  onRetry?: () => void;
}
```

### `ErrorRateLimited`

**When to show:** The API returns 429 Too Many Requests.

**Content:**
- Icon: Bolt (⚡)
- Title: "Rate limited"
- Description: Tells the user to wait. If `retryAfter` is provided, shows the exact number of seconds.
- Optional "Retry now" action.

**Props:**
```tsx
interface ErrorRateLimitedProps {
  retryAfter?: number | null;  // Seconds from Retry-After header
  onRetry?: () => void;
}
```

### `Error5xx`

**When to show:** The API returns a 5xx server error.

**Content:**
- Icon: Error (❌)
- Title: "Server error (500)" (status code is dynamic)
- Description: Includes the request ID for support/debugging.
- Optional "Retry" action.

**Props:**
```tsx
interface Error5xxProps {
  requestId: string | null;    // From x-request-id response header
  statusCode?: number;         // Defaults to 500
  onRetry?: () => void;
}
```

### Generic `ErrorState`

All error components are built on the `ErrorState` primitive:

```tsx
interface ErrorStateProps {
  icon?: React.ReactNode;      // Override the default icon
  title: string;               // Heading text
  description: string;         // Body text
  retryLabel?: string;         // Optional button label
  onRetry?: () => void;        // Optional button handler
}
```

---

## 4. Icons

All icons are simple, monochrome SVGs consistent with the brand. They are exported from `dashboard/src/components/Icons.tsx`.

| Icon | Component | Use |
|------|-----------|-----|
| `IconEmpty` | Empty document with lines | Generic empty state |
| `IconError` | Circle with X | Generic error |
| `IconWarning` | Triangle with exclamation | Warnings |
| `IconLoading` | Spinning circle | Loading state |
| `IconClock` | Clock face | "No events" / catching up |
| `IconFilter` | Funnel | "No filter results" |
| `IconSubscriptions` | Document with bullet | "No subscriptions" |
| `IconServer` | Server rack | "Server unreachable" |
| `IconLock` | Lock | "Unauthorized" |
| `IconRetry` | Circular arrows | Retry action |
| `IconBolt` | Lightning bolt | "Rate limited" |

---

## 5. API Error Handling

The `dashboardApi` client in `dashboard/src/api/client.ts` now throws `ApiError` instances that carry structured error details:

```ts
interface ApiErrorDetail {
  status: number;          // HTTP status code
  statusText: string;      // HTTP status text
  requestId: string | null; // From x-request-id header
  retryAfter: number | null; // From Retry-After header (seconds)
  message: string;         // Error message
}
```

This allows error components to display the correct information (e.g., showing `Retry-After` seconds for rate-limited requests, or a request ID for 5xx errors).

---

## 6. Implementation Checklist

- [x] Skeleton loaders for tables, cards, and charts
- [x] Empty states: no events (with lag), no filter results, no subscriptions
- [x] Error states: unauthorized, server unreachable, rate-limited (with Retry-After), 5xx with request ID
- [x] Simple SVG icons consistent with the brand
- [x] Documented patterns in `STATE_PATTERNS.md`
- [x] CSS styles for all components
- [x] API client enhanced with `ApiError` class
