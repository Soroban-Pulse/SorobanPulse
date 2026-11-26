# SorobanPulse Brand Guidelines

Brand assets live in [`docs/design/brand/`](brand/).

| File | Use |
|------|-----|
| `icon.svg` | Primary icon mark (filled tile) — favicons, avatars, GitHub profile |
| `icon-mono.svg` | Single-colour mark using `currentColor` — VS Code Activity Bar, inline UI |
| `wordmark.svg` | Icon + "SorobanPulse" wordmark — docs header, README, slides |
| `export.sh` | Generates 16/32/64/128/256/512 px PNGs, `favicon.ico` and `apple-touch-icon.png` |

## The mark

A pulse (heartbeat) line crossing an orbital ellipse: the *pulse* of events moving
through the Soroban network. The cyan dot is the event being indexed.

## Colours

| Token | Hex | Usage |
|-------|-----|-------|
| Pulse Indigo | `#4F46E5` | Primary brand colour, icon tile, links, primary buttons |
| Indigo 200 | `#C7D2FE` | Orbit ring, subtle accents on indigo |
| Signal Cyan | `#22D3EE` | Highlights, live/streaming indicators |
| Ink | `#111827` | Wordmark text, body text on light backgrounds |
| Paper | `#FFFFFF` | Backgrounds, pulse line on the tile |

Status colours (success/warning/danger) come from the design tokens in
[`design-tokens.md`](../design-tokens.md) and are never used in the logo.

## Typography

- **Headings & wordmark:** Inter, 700 weight (fallback: `system-ui, sans-serif`).
- **Body:** Inter, 400/500.
- **Code, IDs, hashes:** JetBrains Mono (fallback: `ui-monospace, monospace`).

## Usage rules

- Keep clear space around the mark of at least 25% of its height.
- Minimum size: 16 px for the icon, 96 px wide for the wordmark.
- Do not recolour, stretch, rotate, add shadows, or place the filled icon on busy imagery.
- On dark backgrounds use the filled icon as-is, or `icon-mono.svg` in white.
- "SorobanPulse" is one word with a capital S and P.
