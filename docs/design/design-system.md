# SorobanPulse design system

A single visual language shared by the web dashboard (`web/`), the docs site, the VS Code
extension webviews, the embeddable feed widget and Swagger UI.

| Piece | Where |
|---|---|
| Design tokens (source) | [`design/tokens/`](../../design/tokens/), in Style Dictionary JSON |
| Tokens (generated) | [`design/build/`](../../design/build/): `tokens.css`, `tokens.json`, `tokens.dark.json`, `tokens.js` |
| Component CSS | [`design/components.css`](../../design/components.css) |
| Brand assets | [`docs/assets/brand/`](../assets/brand/) |
| Figma library | **Not published yet.** A maintainer with access to the SorobanPulse Figma team should add the link here. Build the library from the specs below, and import `design/build/tokens.json` with the Tokens Studio plugin so Figma and code share the same values. |

## Principles

1. **Data first.** Events, hashes and ledgers are the product. Chrome stays quiet, using neutral
   surfaces, one accent colour, and colour only where it carries meaning.
2. **Dense but calm.** Operators scan long tables. Default text is 14 px, rows are 44 px, and hashes
   and IDs are set in a monospace font with tabular numbers.
3. **Both themes are first-class.** Every semantic token has a light and a dark value. Never
   hard-code a hex colour in a component.
4. **Accessible by default.** Text meets WCAG 2.2 AA contrast, focus is always visible, colour is
   never the only signal, and motion respects `prefers-reduced-motion`.

## Tokens

Tokens come in two tiers:

- **Base tokens** (`tokens/base/`) are raw values: the colour palette, type scale, spacing, radius,
  sizes and motion. They are the same in every theme.
- **Semantic tokens** (`tokens/themes/light.json`, `tokens/themes/dark.json`) describe a *role*,
  such as `color.bg.surface` or `color.error.fg`, and reference base tokens. Both theme files define
  exactly the same keys.

**Components use semantic tokens only.** Palette tokens such as `--sp-color-violet-600` exist so
themes can reference them, not so components can.

Every token becomes a CSS custom property with the `sp` prefix. For example, `color.bg.surface`
becomes `--sp-color-bg-surface`, and `font.size.md` becomes `--sp-font-size-md`.

### Colour

| Semantic group | Tokens | Use |
|---|---|---|
| `bg` | `canvas`, `surface`, `raised`, `subtle`, `hover`, `selected`, `inverse`, `overlay` | Page background → cards → drawers and popovers; row states; drawer backdrop |
| `text` | `primary`, `secondary`, `muted`, `inverse`, `on-accent`, `link` | Body → labels → hints and placeholders |
| `border` | `default`, `strong`, `subtle` | Card outlines → input borders → table row dividers |
| `accent` | `default`, `hover`, `active`, `subtle`, `fg` | Primary buttons, selected tabs, and accent text on the canvas |
| `focus` | `ring` | 2 px focus outline, offset by 2 px |
| `success` · `warn` · `error` · `info` | `fg`, `bg`, `border`, `solid` | Status badges, toasts, inline validation. Use `fg` on `bg` for text, `solid` for icons, dots and fills. |
| `event` | `contract`, `system`, `diagnostic` | Soroban event-type badges |
| `code` | `bg`, `key`, `string`, `number`, `literal` | JSON and code blocks |
| `brand` | `violet`, `cyan`, `ink`, `gradient` | Logo and marketing only, never for UI state |

Status colours always come with a label or icon. For example, a "Live" badge has a dot *and* the
word.

### Theming

`tokens.css` defines the light theme on `:root` and overrides the semantic tokens for dark mode in
two situations:

```css
:root[data-theme="dark"] { … }                     /* explicit user choice */
@media (prefers-color-scheme: dark) {
  :root:not([data-theme="light"]) { … }            /* follows the OS unless the user chose light */
}
```

To force a theme, set `data-theme="light"` or `data-theme="dark"` on `<html>`. Remove the attribute
to follow the OS. Set `color-scheme: var(--sp-color-scheme)` on the root so native form controls and
scrollbars match (the `.sp-root` class does this for you).

### Typography

| Token | Size | Use |
|---|---|---|
| `font.size.xs` | 12 px | Table headers (uppercase, `letter-spacing.caps`), captions, badges |
| `font.size.sm` | 13 px | Dense UI, code, hashes |
| `font.size.md` | 14 px | **Default UI text** |
| `font.size.lg` | 16 px | Docs body copy |
| `font.size.xl` | 18 px | Section and drawer titles |
| `font.size.2xl` | 22 px | Page titles |
| `font.size.3xl` | 28 px | Stat card numbers |
| `font.size.4xl` | 36 px | Marketing headings |

The fonts are `font.family.sans` (Inter, falling back to the system UI font) and `font.family.mono`
(JetBrains Mono, falling back to the system monospace font). Use weights 400 for body text, 500 for
labels and buttons, 600 for headings, and 700 only in the wordmark. Line heights are `tight` 1.2 for
headings, `normal` 1.5 for body text, and `code` 1.6 for code.

Always set contract IDs, transaction hashes, ledger hashes and JSON in the monospace font. Truncate
long values in the middle (`CDLZFC…CYSC`), and show the full value in a `title` attribute and in the
detail view.

### Spacing, radius, size

- **Spacing** uses a 4 px grid: `space.1` = 4 px … `space.16` = 64 px, plus `space.0-5` = 2 px for
  hairline gaps. Use `space.4` (16 px) for page gutters, `space.5` for card padding, and `space.3`
  between form controls.
- **Radius:** `sm` 4 px for badges and inline code · `md` 6 px for buttons, inputs and selects ·
  `lg` 10 px for cards, tables, toasts and code blocks · `xl` 16 px for drawers and modals · `full`
  for pills and dots.
- **Control heights:** `size.control.sm` 28 px, `md` 36 px (default), `lg` 44 px (use this for
  touch-first layouts; it meets the 44 px target size).

### Elevation

| Token | Use |
|---|---|
| `elevation.0` | Flat, inside another surface |
| `elevation.1` | Cards, tables |
| `elevation.2` | Dropdowns, popovers |
| `elevation.3` | Toasts |
| `elevation.4` | Drawers, modals |

Dark-theme shadows are stronger and add a 1 px light hairline, because a plain shadow barely
shows on dark surfaces. Pair elevation with the matching background: `bg.surface` for 1, and
`bg.raised` for 2–4.

### Motion

| Token | Value | Use |
|---|---|---|
| `motion.duration.fast` | 100 ms | Hover, press, focus colour changes |
| `motion.duration.normal` | 180 ms | Tabs, toggles, small reveals |
| `motion.duration.slow` | 280 ms | Drawers and toasts entering |
| `motion.duration.pulse` | 1600 ms | Live-indicator heartbeat (loop) |
| `motion.easing.standard` | `cubic-bezier(0.2, 0, 0, 1)` | Most transitions |
| `motion.easing.enter` / `exit` | decelerate / accelerate | Elements appearing / leaving |

Animate only `opacity`, `transform`, colours and `box-shadow`. Under `prefers-reduced-motion: reduce`,
all animation is collapsed to a single 1 ms frame (see `components.css`).

## Components

Every component below is implemented in `design/components.css` as `.sp-<name>`. The Figma library
should mirror the same names, variants and states.

### Button (`.sp-btn`)

- **Variants:** `--primary` (one per view, for the main action) · `--secondary` (outlined) · `--ghost`
  (toolbar and icon actions) · `--danger` (destructive, always confirmed).
- **Sizes:** `--sm` 28 px · default 36 px · `--lg` 44 px · `--icon` (square; must have an
  `aria-label`).
- **States:** hover, active, focus-visible (ring), disabled (50 % opacity, `not-allowed`), and
  toggled (`aria-pressed="true"` uses the accent-subtle fill).
- Labels are verbs in sentence case ("Go live", "Copy JSON"). Don't use all caps.

### Input and select (`.sp-input`, `.sp-select`, inside `.sp-field`)

- Always pair with a visible `.sp-field__label`. Put help text in `.sp-field__hint`, and errors in
  `.sp-field__error` together with `aria-invalid="true"` on the control.
- Use `.sp-mono` for inputs that take IDs or hashes.
- A select uses the native `<select>` element with a token-coloured chevron. Don't build custom
  listboxes unless search is needed.

### Table (`.sp-table` inside `.sp-table-wrap`)

- The header is sticky, uppercase `xs` text on `bg.subtle`. Rows are `md` text with `border.subtle`
  dividers.
- Use tabular figures for numeric columns (`.sp-table__num`).
- Clickable rows get `tabindex="0"`, open on Enter or Space, and show `aria-selected="true"` while
  their detail drawer is open.
- An empty state is a single full-width cell (`.sp-table__empty`) that explains *why* the table is
  empty and what to do next.
- The wrapper scrolls horizontally on narrow screens, so the page itself never does.

### Badge (`.sp-badge`)

- **Status:** `--success`, `--warn`, `--error`, `--info`, `--accent`.
- **Event type:** `--contract`, `--system`, `--diagnostic`.
- Combine with `.sp-dot` for live or connection state; `.sp-dot--live` adds the pulse animation.
- Use badges for status only. Don't make them clickable.

### Toast (`.sp-toast` inside `.sp-toast-region`)

- The region is fixed to the bottom right, has `role="status"` and `aria-live="polite"`, and toasts
  stack upwards.
- **Variants:** default (info), `--success`, `--warn`, `--error`. The colour is carried by a 4 px left
  border.
- Toasts auto-dismiss after 4 s (8 s for errors, which also get `role="alert"`), and always have a
  dismiss button.
- Toasts confirm a result ("Settings saved") or report a background problem. Never put required
  actions in a toast.

### Drawer (`.sp-drawer` + `.sp-drawer-backdrop`)

- Slides in from the right, `min(32rem, 100vw)` wide, on `bg.raised` with `elevation.4`.
- Uses `role="dialog"` with `aria-modal="true"` and a label. Focus moves to the close button on
  open. Escape, the close button and a backdrop click all close it.
- Used for record details, such as the event JSON. Use a page, not a drawer, for flows that need
  more than one step.

### Tabs (`.sp-tabs`, `.sp-tabs__tab`)

- `role="tablist"` / `role="tab"`, with `aria-selected="true"` on the active tab, which gets a 2 px
  accent underline.
- Use at most about five tabs. Labels are nouns ("Events", "Settings").

### Card (`.sp-card`)

- A `bg.surface` card with a `border.default` outline, `radius.lg`, `elevation.1` and `space.5`
  padding.
- For stat cards, use `.sp-card__title` (the label, in secondary text) above `.sp-card__value`
  (`3xl` with tabular numbers).

### Code / JSON block (`.sp-code`)

- `<pre>` in monospace `sm` with a line height of 1.6, on `code.bg`, scrolling on both axes and
  capped at 60 vh.
- Syntax classes: `.tok-key`, `.tok-string`, `.tok-number`, `.tok-literal`. The dashboard's
  `jsonBlock()` builds the spans with DOM APIs (never `innerHTML`), so event payloads can't inject
  markup.
- Offer a "Copy JSON" action next to any block longer than a few lines.

## Using the tokens

### Web dashboard (`web/`)

```ts
import '../../design/build/tokens.css';
import '../../design/components.css';
```

Add `class="sp-root"` on `<body>`, then compose the `.sp-*` classes. The dashboard's layout styles
(`web/src/styles/app.css`) use only `var(--sp-…)` values.

### VS Code webviews

Map SorobanPulse tokens onto the editor theme variables so webviews follow the user's VS Code
theme, and use our tokens only for brand and event-type colours:

```css
:root {
  --sp-color-bg-surface: var(--vscode-editor-background);
  --sp-color-text-primary: var(--vscode-foreground);
  --sp-color-border-default: var(--vscode-panel-border);
  --sp-color-focus-ring: var(--vscode-focusBorder);
}
```

### Swagger UI and the docs site

Load `tokens.css` and override the Swagger UI or docs-theme variables with `var(--sp-…)` values:
`--sp-font-family-sans` for text, `--sp-color-accent-default` for primary buttons, and the `code.*`
tokens for highlighted payloads.

### Other tools

- `design/build/tokens.json` / `tokens.dark.json` hold nested values for tooling, including Figma
  imports through Tokens Studio.
- `design/build/tokens.js` holds ES module constants, for example `ColorBrandViolet`.

## Changing tokens

1. Edit JSON under `design/tokens/`. If you add a semantic token, add it to **both**
   `themes/light.json` and `themes/dark.json`.
2. Rebuild:
   ```bash
   cd design && npm install && npm run build
   ```
3. Commit the source *and* the regenerated `design/build/` files. Consumers use the built files
   directly, without running Style Dictionary.
4. Check contrast for any new text or background pair: at least 4.5:1 for text, and 3:1 for
   borders, focus rings and icons.
