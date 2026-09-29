# SorobanPulse brand assets

The SorobanPulse mark is a **pulse line** (a real-time heartbeat of on-chain events) crossing an
**orbit** with a single **star**, a nod to the Stellar network the project indexes. The mark sits on
a rounded square with a violet-to-cyan gradient (`#5B3DF5` → `#14B8D4`).

![SorobanPulse horizontal logo](logo-horizontal.svg)

## Files

| File | Use |
|---|---|
| `logo-mark.svg` | Primary mark, full colour. Master source for every raster export. |
| `logo-mark-mono.svg` | Single-colour mark using `currentColor`. Use for one-colour print, embossing and icon fonts. |
| `logo-horizontal.svg` / `logo-horizontal-dark.svg` | Mark + wordmark lockup for light / dark backgrounds. Default logo for READMEs, the docs site and the dashboard header. |
| `logo-stacked.svg` | Mark above wordmark, for square placements such as social avatars and slides. |
| `wordmark.svg` / `wordmark-dark.svg` | Wordmark only, for places where the mark already appears nearby. |
| `png/logo-mark-{16…512}.png` | Raster mark at 16, 24, 32, 48, 64, 128, 180, 192, 256 and 512 px. |
| `png/logo-horizontal*-1024.png` | Raster lockups for tools that don't accept SVG. |
| `favicon/` | `favicon.ico` (16/32/48), `favicon.svg`, `apple-touch-icon.png` (180), `icon-192.png`, `icon-512.png`, `site.webmanifest`. |

The VS Code extension uses `vscode-extension/media/icon.png` (the 128 px mark, for the Marketplace)
and `vscode-extension/media/pulse.svg` (a 24 px monochrome `currentColor` icon for the activity bar,
so it follows the user's theme).

### Favicon snippet

```html
<link rel="icon" href="/favicon.ico" sizes="48x48">
<link rel="icon" href="/favicon.svg" type="image/svg+xml">
<link rel="apple-touch-icon" href="/apple-touch-icon.png">
<link rel="manifest" href="/site.webmanifest">
```

## Colours

| Token | Hex | Role |
|---|---|---|
| Pulse Violet | `#5B3DF5` | Gradient start; the "Pulse" half of the wordmark on light backgrounds |
| Signal Cyan | `#14B8D4` | Gradient end |
| Ink | `#0F1222` | The "Soroban" half of the wordmark; dark backgrounds |
| Violet Tint | `#A594FF` | The "Pulse" half of the wordmark on dark backgrounds |
| White | `#FFFFFF` | Pulse line, orbit and star inside the mark |

These are the same values as the `brand.*` tokens in [`design/tokens`](../../../design/tokens/)
(see [the design system](../../design/design-system.md)).

## Typography

The wordmark is set in **Inter**: "Soroban" in Medium (500) and "Pulse" in Bold (700), with −2%
tracking. The letterforms are outlined in the SVGs, so the font does not need to be installed.

## Usage rules

**Clear space.** Keep an empty margin around the logo at least **¼ of the mark's height** on every
side (16 px for a 64 px mark). The gap between mark and wordmark in the horizontal lockup is fixed
and must not be changed.

**Minimum size.**

| Asset | Minimum |
|---|---|
| Mark (screen) | 16 px (use the favicon exports, which are tuned for small sizes) |
| Horizontal lockup (screen) | 120 px wide |
| Horizontal lockup (print) | 30 mm wide |

Below 120 px wide, use the mark on its own.

**Backgrounds.** Use the light lockup on white or light grey, and the `-dark` lockup on `#0F1222` or
similar dark surfaces. On photos or busy backgrounds, use the full-colour mark alone, because its
rounded square carries its own background.

### Don'ts

- Don't recolour the gradient, swap its direction, or replace it with a flat colour (use
  `logo-mark-mono.svg` when you need a single colour).
- Don't stretch, skew, rotate or add effects such as drop shadows, glows or outlines.
- Don't redraw the pulse line or change the number of peaks.
- Don't set the wordmark in a different typeface, or write it as "Soroban Pulse" with a space in
  the logo. In running text, "SorobanPulse" is correct.
- Don't place the light lockup on dark backgrounds, or the dark lockup on light ones.
- Don't combine the mark with other logos inside the clear-space area.
- Don't use the mark to imply endorsement by the Stellar Development Foundation.

## Regenerating

All raster files and lockups are generated from `logo-mark.svg`:

```bash
npm i --no-save @resvg/resvg-js opentype.js @fontsource/inter
node scripts/build-brand-assets.mjs
```

The script also rewrites `vscode-extension/media/icon.png`.
