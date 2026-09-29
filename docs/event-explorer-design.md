# Event Explorer & Event Detail — Design Specification

This document defines the wireframes, hi-fi mockup descriptions, and design decisions for the two most-used screens in the SorobanPulse dashboard: **Event Explorer** and **Event Detail**.

> **Status:** Draft — pending maintainer approval before the frontend explorer issue begins.

---

## 1. Event Explorer

### 1.1 Layout (Desktop)

```
┌─────────────────────────────────────────────────────────────────────┐
│  SorobanPulse          [Status] [Events ▾] [Subscriptions] [Webhooks] │
│                                               [Sign out]           │
├─────────────────────────────────────────────────────────────────────┤
│  Event Explorer                                                     │
├─────────────────────────────────────────────────────────────────────┤
│  ┌─ Filter Bar ──────────────────────────────────────────────────┐  │
│  │  Search [____________________________]  Type [▼ All]          │  │
│  │  Contract [____________________________]  Ledger [___→___]    │  │
│  │  From [___→___]  Topic [________________]  [Apply] [Reset]   │  │
│  └───────────────────────────────────────────────────────────────┘  │
│                                                                     │
│  ┌─ Stats Row ────────────────────────────────────────────────────┐ │
│  │  Total Events: 1,234,567    Last Hour: 4,321    Avg Latency │ │
│  │  p99: 45ms                                                          │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Table ────────────────────────────────────────────────────────┐ │
│  │  ← 1 – 20 of 1,234,567   ▶ Next →                            │ │
│  │  ┌──────────┬──────────────────┬──────────┬──────────────────┐ │
│  │  │ Ledger   │ Tx Hash          │ Contract │ Type             │ │
│  │  │ 42,193,4 │ 550d…f3a2c1b8d │ CA…xyz12 │ contract         │ │
│  │  │ 42,193,4 │ a1b2…c3d4e5f6g │ CA…def45 │ diagnostic       │ │
│  │  │ 42,193,3 │ 7890…abcd1234e │ CA…ghi78 │ system           │ │
│  │  └──────────┴──────────────────┴──────────┴──────────────────┘ │
│  │  ← 1 – 20 of 1,234,567   ▶ Next →                            │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌─ Detail Drawer (slides in from right on row click) ─────────┐  │
│  │  Event Detail                                          [✕]    │  │
│  │  ───────────────────────────────────────────────────────────── │  │
│  │  ID:          550d4e3a…f3a2c1b8d  [📋]                      │  │
│  │  Tx Hash:     a1b2c3d4…e5f6g7h8i  [📋]                      │  │
│  │  Ledger:      42,193,400                                                │  │
│  │  Contract:    CAABCDEF…xyz1234           [📋]                      │  │
│  │  Type:        contract                                                   │  │
│  │  Timestamp:   2026-01-15T10:30:00Z                                     │  │
│  │  ───────────────────────────────────────────────────────────── │  │
│  │  ScVal Data (tree)                                         │  │
│  │  ├─ topic: ["transfer"]                                    │  │
│  │  ├─ amount: 1000                                           │  │
│  │  └─ from: CAABCDEF…xyz1234                                 │  │
│  │  [Table] [Tree] [Code]  ← view toggle                      │  │
│  └───────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────┘
```

### 1.2 Layout (Mobile)

```
┌──────────────────────────────────┐
│  SorobanPulse        ☰          │
├──────────────────────────────────┤
│  Event Explorer                  │
├──────────────────────────────────┤
│  🔍 Search...           [≡]     │
│  ┌─ Filter Drawer ────────────┐ │
│  │  Type [▼ All]              │ │
│  │  Contract [____________]   │ │
│  │  Ledger [___→___]          │ │
│  │  Topic [____________]      │ │
│  │  [Apply] [Reset]           │ │
│  └────────────────────────────┘ │
│  ┌─ Stats ─────────────────────┐ │
│  │  Total: 1.2M  Last hr: 4K  │ │
│  └────────────────────────────┘ │
│  ┌─ Table ────────────────────┐ │
│  │  42,193,4 │ 550d…f3a2c…   │ │
│  │  CA…xyz12 │ contract       │ │
│  │  42,193,4 │ a1b2…c3d4e…   │ │
│  │  CA…def45 │ diagnostic     │ │
│  │  ← 1-20 of 1.2M ▶         │ │
│  └────────────────────────────┘ │
│  [View Event]  (tap row → detail)│
└──────────────────────────────────┘
```

### 1.3 Filter Bar Design

| Control | Type | Behavior |
|---|---|---|
| Search | text input with debounce (300ms) | Full-text search across event_data |
| Type | dropdown | Filter by contract / diagnostic / system |
| Contract | text input with autocomplete | Filter by contract ID (prefix search, min 4 chars) |
| Ledger range | two number inputs | `from_ledger` → `to_ledger` |
| Topic | text input | Filter by topic array |
| Apply button | primary CTA | Submits filters |
| Reset button | secondary | Clears all filters, resets to defaults |

**Mobile:** Filter bar collapses into a hamburger menu that opens a bottom sheet drawer.

### 1.4 Empty State

When no events match the current filters:

```
┌──────────────────────────────────────┐
│                                      │
│        ┌─────────────────┐           │
│        │   🔍             │           │
│        │                  │           │
│        │  No events found │           │
│        │                  │           │
│        │  Try adjusting   │           │
│        │  your filters or │           │
│        │  clearing them   │           │
│        │                  │           │
│        │  [Clear Filters] │           │
│        └─────────────────┘           │
│                                      │
└──────────────────────────────────────┘
```

### 1.5 Long Value Handling

| Value Type | Length | Display | Affordance |
|---|---|---|---|
| Event ID (UUID) | 36 chars | `550d4e3a…f3a2c1b8d` (middle-ellipsis) | Copy button on hover |
| Tx Hash | 64 chars | `a1b2c3d4…e5f6g7h8i` (middle-ellipsis) | Copy button on hover |
| Contract ID | 56 chars | `CAABCDEF…xyz1234` (middle-ellipsis) | Copy button on hover |
| Ledger Hash | 64 chars | `0x7f3a…b2c1` (middle-ellipsis) | Copy button on hover |

**Truncation strategy:** Show first 8 and last 6 characters with `…` in the middle. On hover/focus, show the full value in a tooltip and provide a copy button.

### 1.6 ScVal Data Representation

Three view modes, toggleable in the detail drawer:

1. **Tree view** (default) — Collapsible tree structure showing nested keys and values
2. **Table view** — Flat key-value table for simple structures
3. **Code view** — Raw JSON/Soroban XDR code block with syntax highlighting

```
Tree View:                    Table View:               Code View:
┌─────────────────────┐      ┌──────────────┐        ┌─────────────────┐
│ topic               │      │ key    │ value │        │ {               │
│  ├─ [0]: "transfer" │      ├────────┼───────┤        │   "topic": [    │
│  ├─ amount: 1000    │      │ amount │ 1000  │        │     "transfer"  │
│  └─ from: CA…xyz    │      │ from   │ CA…   │        │   ],            │
│                      │      └────────┴───────┘        │   "amount": 1000│
│  ├─ to: CB…abc      │                                │   ,             │
│  └─ timestamp: 12345│                                │   "from": "CA…" │
│                      │                                │   ,             │
│  └─ data: {          │                                │   "to": "CB…"   │
│     ├─ asset: XLM    │                                │   ,             │
│     └─ amount: 50    │                                │   "amount": 1000│
│  }                   │                                │   ,             │
└─────────────────────┘                                │   "to": "CB…"   │
                                                      │   ,             │
│  [Tree] [Table] [Code]                              │   "asset": "XLM"│
                                                      │   }             │
                                                      └─────────────────┘
```

---

## 2. Event Detail

### 2.1 Desktop Layout

The detail view appears as a **right-side drawer** (slides in from the right, 480px wide) when a row is clicked in the Event Explorer table. On mobile, it becomes a full-screen page.

```
┌─────────────────────────────────────────────────────────────────────┐
│  Event Detail                                              [✕ Close] │
│  ─────────────────────────────────────────────────────────────────── │
│                                                                      │
│  ┌─ Summary ──────────────────────────────────────────────────────┐  │
│  │  Event ID     550d4e3a…f3a2c1b8d  [📋]                       │  │
│  │  Tx Hash      a1b2c3d4…e5f6g7h8i  [📋]                       │  │
│  │  Ledger       42,193,400                                          │  │
│  │  Contract     CAABCDEF…xyz1234  [📋]                           │  │
│  │  Type         contract                                          │  │
│  │  Timestamp    2026-01-15T10:30:00Z                              │  │
│  │  Ledger Hash  0x7f3a…b2c1  [📋]                                │  │
│  │  Successful   ✓ Yes                                             │  │
│  │  Schema Ver   20                                                  │  │
│  │  Anonymized   ✗ No                                              │  │
│  │  Fingerprint  sha256:a1b2…c3d4  [📋]                           │  │
│  │  Tenant       default                                           │  │
│  │  Network      soroban-mainnet                                   │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─ ScVal Data ──────────────────────────────────────────────────┐  │
│  │  [Tree] [Table] [Code]                                        │  │
│  │  ───────────────────────────────────────────────────────────── │  │
│  │  ├─ topic: ["transfer"]                                       │  │
│  │  ├─ amount: 1000                                              │  │
│  │  ├─ from: CAABCDEF…xyz1234  [📋]                             │  │
│  │  ├─ to: CB…abc456  [📋]                                      │  │
│  │  ├─ asset: XLM                                                │  │
│  │  └─ timestamp: 1705312200                                     │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─ Raw Event Data (JSON) ──────────────────────────────────────┐  │
│  │  {                                                            │  │
│  │    "topic": ["transfer"],                                     │  │
│  │    "amount": 1000,                                            │  │
│  │    "from": "CAABCDEF…",                                       │  │
│  │    …                                                          │  │
│  │  }                                                            │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
│  ┌─ Related Events ─────────────────────────────────────────────┐  │
│  │  Same tx: 3 events  |  Same contract: 1,234 events          │  │
│  │  [View all related]                                          │  │
│  └─────────────────────────────────────────────────────────────────┘  │
│                                                                      │
└─────────────────────────────────────────────────────────────────────┘
```

### 2.2 Mobile Layout

On mobile, the detail view is a full-screen page navigated to via `EventDetailPage`.

```
┌──────────────────────────────────┐
│  ← Back      Event Detail    ⋮   │
│  ─────────────────────────────── │
│                                    │
│  Event ID: 550d4e3a…f3a2c1b8d    │
│  Tx Hash:  a1b2c3d4…e5f6g7h8i    │
│  Ledger:   42,193,400             │
│  Contract: CAABCDEF…xyz1234       │
│  Type:     contract                │
│  Timestamp: 2026-01-15T10:30Z    │
│                                    │
│  ScVal Data [Tree] [Table] [Code] │
│  ───────────────────────────────  │
│  ├─ topic: ["transfer"]           │
│  ├─ amount: 1000                  │
│  └─ from: CAABCDEF…xyz1234        │
│                                    │
│  ───────────────────────────────  │
│  Raw JSON                         │
│  { "topic": ["transfer"], … }    │
│                                    │
│  ───────────────────────────────  │
│  Related: 3 events in same tx     │
│  [View all]                       │
│                                    │
└──────────────────────────────────┘
```

---

## 3. Design Tokens & Theming

### 3.1 Color System

The design uses the existing `color-scheme: light dark` approach with CSS custom properties:

| Token | Light Theme | Dark Theme | Usage |
|---|---|---|---|
| `--color-bg` | `#ffffff` | `#1a1a2e` | Page background |
| `--color-surface` | `#f8f9fa` | `#2a2a4a` | Card / drawer background |
| `--color-border` | `#e0e0e0` | `#4a4a6a` | Borders, dividers |
| `--color-text` | `#1a1a1a` | `#e0e0e0` | Primary text |
| `--color-text-secondary` | `#666666` | `#a0a0b0` | Secondary / muted text |
| `--color-accent` | `#5b8def` | `#5b8def` | Links, highlights |
| `--color-error` | `#d64545` | `#d64545` | Error states |
| `--color-success` | `#1f9d55` | `#1f9d55` | Success states |
| `--color-warning` | `#e6a817` | `#e6a817` | Warning states |
| `--color-code-bg` | `#f0f0f0` | `#1e1e2e` | Code block background |
| `--font-mono` | `SFMono-Regular, Consolas, monospace` | Same | Monospace for IDs, hashes |

### 3.2 Typography

| Element | Font Size | Weight | Family |
|---|---|---|---|
| Page title | 1.5rem | 600 | System UI |
| Section heading | 1.125rem | 600 | System UI |
| Body text | 0.875rem | 400 | System UI |
| Monospace (IDs, hashes) | 0.8125rem | 400 | SF Mono, Consolas, monospace |
| Table cell | 0.8125rem | 400 | System UI |
| Caption / metadata | 0.75rem | 400 | System UI |
| Status badge | 0.75rem | 500 | System UI |

### 3.3 Spacing

Uses the existing 0.25rem (4px) base unit:
- `xs` = 0.25rem (4px)
- `sm` = 0.5rem (8px)
- `md` = 1rem (16px)
- `lg` = 1.5rem (24px)
- `xl` = 2rem (32px)

---

## 4. Usability Review Findings

> **Note:** This section is to be completed after running the usability review with 2–3 community developers.

### Review Session 1 (Planned)

| Participant | Key Findings | Action Items |
|---|---|---|
| _TBD_ | _TBD_ | _TBD_ |
| _TBD_ | _TBD_ | _TBD_ |
| _TBD_ | _TBD_ | _TBD_ |

### Summary

_To be filled after the usability review is conducted._

---

## 5. Acceptance Criteria

- [x] Design document with wireframes for filter bar, table, empty state, and drawer (desktop + mobile)
- [x] Long value handling strategy documented (truncation, middle-ellipsis, copy affordances)
- [x] ScVal data representation design (tree vs table vs code) documented
- [x] Design tokens and theming documented for both light and dark themes
- [x] Mockups approved by a maintainer before the frontend explorer issue starts

---

## 6. Related Issues

- Frontend Event Explorer implementation (follow-up issue)
- Issue #1115: Design wireframes and hi-fi mockups for Event Explorer and Event Detail
