# Contributing to the dashboard

This guide covers the frontend in `dashboard/`. For the project as a whole see
the root [CONTRIBUTING.md](../CONTRIBUTING.md); for architecture and the API
contract see [docs/web-dashboard.md](../docs/web-dashboard.md).

```bash
cd dashboard
npm install
npm run dev     # http://localhost:5173, proxies /api to :8080
npm run lint    # i18n + accessibility lint rules
npm run build   # type-check and production build
```

## Accessibility (WCAG 2.2 AA)

On-call operators use the dashboard with keyboards, screen readers and phones.
Every change should keep these guarantees:

### Keyboard

- Everything interactive must be a real `<button>`, `<a href>`, `<input>`,
  `<select>` or `<summary>`. Don't attach `onClick` to a `div` or `tr`; the
  `jsx-a11y` lint rules will flag it.
- Focus is always visible. The global `:focus-visible` outline in `styles.css`
  covers this; don't remove outlines on individual components.
- A "Skip to main content" link is the first tab stop. After client-side
  navigation, focus moves to `<main>`.
- Drawers (the mobile navigation and `DetailDrawer`) use `useFocusTrap`: focus
  moves inside on open, Tab cycles within, Escape closes, and focus returns to
  the control that opened it. Reuse this hook for any new modal surface.
- Scrollable regions (`.table-scroll`, `<pre>`) get `tabIndex={0}` plus a label
  so keyboard users can scroll them.

### Labels and structure

- Each page has exactly one `<h1>`; sections use `<h2>`/`<h3>` in order.
- Every form control has a visible `<label htmlFor>`. Placeholders are not labels.
- Filter bars are `<form role="search" aria-label=…>`.
- Tables use `<caption>`, `<th scope="col">` and a `<th scope="row">` for the
  primary column. Use `ResponsiveTable`, which does this for you.
- Icon-only buttons need an `aria-label`. The SVG icons in `Icons.tsx` are
  `aria-hidden`.
- Status is never conveyed by colour alone. Badges carry text, and delivery
  codes read "503 (failed)".

### Charts

Recharts output is not accessible on its own. Wrap charts in `ChartFigure`,
which provides:

1. a `role="img"` container whose `aria-label` summarises the series (latest
   and peak values), and
2. an expandable "Show data table" disclosure with every data point.

### Live updates

- Use `LiveRegion` (a polite `role="status"` region) for background updates.
  Never use `aria-live="assertive"` for routine data.
- Summarise updates instead of announcing each one: the status page announces
  only health *changes*, and the live stream announces "N new events" at most
  every 10 seconds.
- Anything that updates continuously must be pausable. The live stream has a
  Pause button, marked with `aria-pressed`.

### Colour and themes

All colours are tokens at the top of `styles.css`, defined for light and dark
themes (the system preference or an explicit choice in the sidebar). Measured
contrast ratios:

| Token | Light (on bg / surface) | Dark (on bg / surface) | Needs |
|---|---|---|---|
| `--text` | 16.5 / 15.3 | 15.3 / 13.4 | 4.5 |
| `--text-muted` | 7.5 / 7.0 | 8.8 / 7.7 | 4.5 |
| `--accent` (links, focus) | 6.7 / 6.2 | 8.9 / 7.8 | 4.5 |
| `--ok` | 6.5 / 6.0 | 10.6 / 9.3 | 4.5 |
| `--error` | 6.5 / 6.0 | 8.1 / 7.1 | 4.5 |
| `--border` (inputs) | 3.9 / 3.6 | 4.0 / 3.5 | 3.0 |
| `--accent-text` on `--accent` | 6.7 | 8.9 | 4.5 |
| badges (white on active / paused / failing) | 6.5 / 6.9 / 6.5 | same | 4.5 |

If you add or change a token, re-measure the pair in both themes (for example
with the WebAIM contrast checker or DevTools) and update this table.

### Motion

Transitions are disabled under `prefers-reduced-motion: reduce`.

## Responsive layout

The dashboard must work at 375px wide with no horizontal page scroll.

- There is one breakpoint, `767.98px`, shared by `styles.css` and
  `MOBILE_QUERY` in `src/hooks/useMediaQuery.ts`. Below it:
  - the sidebar becomes an off-canvas drawer opened from the top bar,
  - `ResponsiveTable` swaps its `<table>` for a card list showing the first
    column plus every column marked `key: true`,
  - `DetailDrawer` goes full-screen.
- Touch targets (buttons, links, inputs, selects, `<summary>`) are at least
  44×44px via `--touch`.
- Long unbroken values (contract IDs, URLs, hashes) get `overflow-wrap: anywhere`
  (`.mono` does this). Don't give anything a fixed width wider than the screen;
  use `min(…, 100%)`.
- Grid and flex children that hold text need `min-width: 0` so they can shrink.

## Internationalisation

All user-facing text comes from `src/locales/<lang>.json` through
`react-i18next`. `npm run lint` fails on hard-coded strings in JSX
(`i18next/no-literal-string`), including `aria-label`, `title`, `alt`,
`placeholder` and `label` attributes.

```tsx
const { t } = useTranslation();
<h1>{t("subscriptions.heading")}</h1>
<p>{t("subscriptions.resultCount", { count, total })}</p>  // plurals: key_one / key_other
```

Format numbers, dates and durations with `useFormat()` from
`src/i18n/format.ts` (Intl-based, using the selected locale and the browser's
timezone). Don't use `toLocaleString()` without a locale, and don't build
sentences by string concatenation.

### Adding a language

1. Copy `src/locales/en.json` to `src/locales/<code>.json` (a BCP 47 base
   language such as `fr` or `pt`) and translate the values. Keep the keys and
   the `{{placeholders}}` unchanged. Add the plural forms your language needs
   (`_zero`, `_one`, `_two`, `_few`, `_many`, `_other`; see
   [i18next plurals](https://www.i18next.com/translation-function/plurals)).
2. In `src/i18n/index.ts`, import the file, add it to `resources`, and add an
   entry to `SUPPORTED_LOCALES` with the language's own name for itself
   (`{ code: "fr", nativeName: "Français" }`).
3. Run `npm run dev`, choose the language in the sidebar, and check each
   screen at 375px for overflow. Translations are often longer than English.
4. Open a PR. Ask a second native speaker to review the wording if you can.

Missing keys fall back to English, so a partial translation is safe to merge.
