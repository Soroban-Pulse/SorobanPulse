# Data-Visualisation Guidelines

> **Status:** Design contract (draft). This document defines how SorobanPulse
> presents charts so every chart looks and behaves consistently across the
> dashboard and any future frontend pages.

## Table of Contents

1. [Contract page layout](#contract-page-layout)
2. [Chart guidelines](#chart-guidelines)
   - [Categorical palette (colour-blind safe)](#categorical-palette-colour-blind-safe)
   - [Axis formatting](#axis-formatting)
   - [Tooltips](#tooltips)
   - [Empty and low-data states](#empty-and-low-data-states)
   - [Large-number formatting](#large-number-formatting)
3. [Displaying i128 token amounts and decimals](#displaying-i128-token-amounts-and-decimals)
4. [Referencing this document from frontend issues](#referencing-this-document-from-frontend-issues)

---

## Contract page layout

The contract detail page is composed of five sections, rendered top-to-bottom:

### 1. Header

| Element | Description |
|---------|-------------|
| Page title | Contract ID (truncated with ellipsis if long), e.g. `CA…3FZ` |
| Breadcrumb | `Contracts` › `CA…3FZ` |
| Status badge | `active` / `paused` / `failing` — uses the same `.status-badge` classes as the subscriptions page |
| Copy button | One-click copy of the full contract ID to clipboard |

### 2. KPI cards

A horizontal row of 3–4 summary tiles (reuse the existing `StatTile` component pattern):

| Tile | Value source | Format |
|------|-------------|--------|
| Total events | `events.count` | Integer with thousands separator |
| 24h events | `events.last_24h` | Integer |
| Avg events / hour | `events.avg_hourly` | Decimal (1 dp) |
| Last event | `events.latest_timestamp` | Relative time ("3m ago") or ISO timestamp |

Each card has:
- A label in small, muted text (`font-size: 0.75rem`, opacity 0.7)
- A value in large, bold text (`font-size: 1.5rem`, weight 600)
- A subtle border and rounded corners (`border-radius: 0.5rem`)

### 3. Activity chart (time series)

A line chart showing event ingestion over time.

| Property | Guideline |
|----------|-----------|
| Chart type | Line chart (`type="monotone"` in Recharts) |
| X-axis | Time bucket (hourly by default, configurable) |
| Y-axis | Event count, left-aligned, starts at 0 |
| Line colour | Use the designated series colour from the [palette](#categorical-palette-colour-blind-safe) |
| Dot markers | Hidden (`dot={false}`) for dense series; shown on hover via tooltip |
| Height | 300 px |
| Responsive | Wrapped in `<ResponsiveContainer width="100%" height={300}>` |

### 4. Event-type breakdown

A bar or donut chart showing the distribution of event types for the selected contract.

| Property | Guideline |
|----------|-----------|
| Chart type | Horizontal bar chart (preferred) or donut chart |
| Colour mapping | Each event type maps to a colour from the [palette](#categorical-palette-colour-blind-safe) |
| Sorting | Bars sorted descending by count |
| Labels | Event type name + count on the bar segment |
| Height | 250 px |

### 5. Event table

A paginated table of raw events with the following columns:

| Column | Content | Format |
|--------|---------|--------|
| Timestamp | ISO 8601, localised to the user's timezone | `YYYY-MM-DD HH:mm:ss Z` |
| Transaction hash | Truncated (first 8 + last 6 chars) with full hash on hover | `0x1a2b…c3d4` |
| Event type | Raw event name string | Plain text |
| Ledger sequence | Integer | Thousands separator |
| Details | JSON summary (collapsed by default) | Expandable row |

---

## Chart guidelines

### Categorical palette (colour-blind safe)

Use the following 8-colour palette, which is designed to be distinguishable
under the three most common forms of colour vision deficiency (protanopia,
deuteranopia, tritanopia):

| Token | Hex | Use |
|-------|-----|-----|
| Blue | `#5b8def` | Primary series (events ingested, throughput) |
| Orange | `#e0725b` | Secondary series (latency, errors) |
| Teal | `#2a9d8f` | Tertiary series (success rate, delivery rate) |
| Purple | `#7b68ae` | Quaternary series (queue depth, pending) |
| Olive | `#8a9a3b` | Fifth series |
| Coral | `#e06b6b` | Error / failure indicators |
| Slate | `#6b7b8d` | Neutral / muted elements |
| Gold | `#d4a03c` | Warning / caution indicators |

**Rules:**
- Never rely on colour alone. Always pair colour with a shape (line style,
  dot pattern) or a direct label.
- When more than 8 categories are needed, cycle the palette and differentiate
  by line style (solid, dashed, dotted).
- Background and text must maintain at least a 4.5:1 contrast ratio
  (WCAG AA).

### Axis formatting

- **X-axis (time):** Use human-readable, abbreviated labels (`HH:mm`, `MM-DD`
  or `YYYY-MM-DD` depending on the zoom level). Rotate labels 45° when they
  would overlap.
- **Y-axis (numeric):**
  - Always start at 0 for bar/area charts.
  - For line charts, use a `nice` domain that rounds to the next major tick.
  - Show grid lines at every major tick, faint grey (`rgba(128,128,128,0.2)`).
  - Tick font size: 10 px.
- **Axis labels:** Include a unit label on each axis (e.g. "Events", "ms",
  "USD"). Use sentence case, no colon.

### Tooltips

- Show on hover (desktop) and tap (mobile).
- Format:
  ```
  2026-06-15 14:00 UTC
  Events ingested: 1,234
  p99 latency: 45 ms
  ```
- Use the series colour as a left-border accent in the tooltip.
- Do not show raw timestamps in tooltips; use the same human-readable format
  as the axis labels.
- Keep tooltip content to ≤ 4 lines; truncate with ellipsis if longer.

### Empty and low-data states

| State | Visual treatment |
|-------|-----------------|
| No data (empty dataset) | Show a centred message inside the chart area: "No data available for the selected period." Use the slate colour (`#6b7b8d`). |
| Sparse data (< 3 points) | Render the chart but add a subtle annotation: "Low data — trends may not be meaningful." Do not hide the chart. |
| Loading | Show a skeleton placeholder with the same chart dimensions and a subtle pulse animation. |
| Error | Show an error message inside the chart area: "Failed to load chart data." + a Retry button. |

### Large-number formatting

- Use SI-prefix abbreviations for numbers ≥ 10,000:
  - `1,234` → `1,234`
  - `10,000` → `10k`
  - `1,000,000` → `1M`
  - `1,000,000,000` → `1B`
- Always include thousands separators (commas) for numbers < 10,000.
- Never truncate significant digits. Round to at most 1 decimal place for
  abbreviated forms (e.g. `1.2M`, `3.4k`).
- For currency values, always show 2 decimal places (e.g. `$1,234.56`).

---

## Displaying i128 token amounts and decimals

Soroban smart contracts use `i128` integers to represent token balances and
amounts. Because these values are stored as raw integers, they must be
converted to human-readable decimal strings before display.

### Conversion rule

```
display_amount = raw_i128 / 10^decimals
```

Where `decimals` is the token's configured decimal precision (commonly 7 for
Stellar-native tokens, but may vary per contract).

### Formatting rules

1. **Use the token's declared decimals.** Read the `decimals` field from the
   contract's token metadata; do not hard-code a default.
2. **Show at most 6 significant decimal places.** Trim trailing zeros.
   - Example (7 decimals): `123456789` → `123.456789`
   - Example (7 decimals, trailing zeros): `123456700` → `123.4567`
3. **Use thousands separators for the integer part.**
   - Example: `1234567890123456` → `1,234,567,890.123456`
4. **For very large or very small amounts, use SI-prefix notation.**
   - `1,000,000.00` → `1.00M`
   - `0.000001` → `1.00µ` (micro)
5. **Always append the token ticker** (e.g. `USDC`, `XLM`) after the number.
   - `1,234.56 USDC`
6. **Never display the raw `i128` integer to end users.** It is only for
   internal/developer use (e.g. in API responses or debug views).
7. **Handle negative values** (for debit/withdrawal amounts) with a leading
   minus sign: `-1,234.56 USDC`.

### Example table

| Raw i128 | Decimals | Display |
|----------|----------|---------|
| `123456789` | 7 | `123.456789 USDC` |
| `10000000` | 7 | `10 USDC` |
| `1` | 7 | `0.0000001 USDC` |
| `5000000000000000` | 7 | `5,000,000.00 USDC` |
| `-25000000` | 7 | `-25.000000 USDC` |

---

## Referencing this document from frontend chart issues

When filing a frontend issue that involves charts or data visualisation,
reference this document by linking to
`https://github.com/Soroban-Pulse/SorobanPulse/blob/main/docs/design/dataviz.md`
and cite the relevant section (e.g. "palette", "axis formatting", "i128
display"). This ensures consistency across all chart-related work.
