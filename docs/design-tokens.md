# SorobanPulse Design Tokens

This file is the **single source of truth** for every visual constant used across the documentation site, Swagger UI, and any future UI surfaces. When you change a value here, propagate it to:

- `docs/site/src/css/custom.css` — Docusaurus theme overrides
- `src/handlers.rs` — Swagger UI inline CSS
- `docs/diagram-style-guide.md` — Mermaid `%%{init}` theme blocks

---

## Colour Palette

### Brand

| Token | Value | Usage |
|---|---|---|
| `--sp-brand-600` | `#7c3aed` | Primary accent, buttons, links, active nav |
| `--sp-brand-500` | `#8b5cf6` | Hover state for primary |
| `--sp-brand-400` | `#a78bfa` | Light accent, headings on dark bg |
| `--sp-brand-300` | `#c4b5fd` | Subtle tint, badge backgrounds |
| `--sp-brand-100` | `#ede9fe` | Very light tint on white bg |

### Neutrals (Dark surface — default)

| Token | Value | Usage |
|---|---|---|
| `--sp-bg` | `#0f1117` | Page background |
| `--sp-surface` | `#1a1d2e` | Cards, sidebar, header |
| `--sp-surface-raised` | `#222540` | Elevated cards, dropdowns |
| `--sp-border` | `#2d3158` | Dividers, input borders |
| `--sp-border-subtle` | `#1e2240` | Faint dividers |

### Text (Dark)

| Token | Value | Usage |
|---|---|---|
| `--sp-text` | `#e2e8f0` | Primary prose |
| `--sp-text-muted` | `#94a3b8` | Secondary/meta text |
| `--sp-text-faint` | `#64748b` | Disabled, placeholder |

### Neutrals (Light surface — light mode)

| Token | Value | Usage |
|---|---|---|
| `--sp-bg-light` | `#ffffff` | Page background |
| `--sp-surface-light` | `#f8fafc` | Cards, sidebar |
| `--sp-surface-raised-light` | `#f1f5f9` | Elevated elements |
| `--sp-border-light` | `#e2e8f0` | Dividers |
| `--sp-text-light` | `#0f172a` | Primary prose |
| `--sp-text-muted-light` | `#475569` | Secondary text |

### Semantic

| Token | Value | Usage |
|---|---|---|
| `--sp-success` | `#10b981` | GET badges, success callouts |
| `--sp-success-bg` | `#064e3b` | Success admonition background (dark) |
| `--sp-warning` | `#f59e0b` | PATCH badges, warning callouts |
| `--sp-warning-bg` | `#451a03` | Warning admonition background (dark) |
| `--sp-danger` | `#ef4444` | DELETE badges, error callouts |
| `--sp-danger-bg` | `#450a0a` | Danger admonition background (dark) |
| `--sp-info` | `#38bdf8` | Info callouts, note admonitions |
| `--sp-info-bg` | `#082f49` | Info admonition background (dark) |

---

## Typography

### Font Stacks

| Token | Value | Usage |
|---|---|---|
| `--sp-font-sans` | `Inter, -apple-system, BlinkMacSystemFont, "Segoe UI", system-ui, sans-serif` | All UI text and prose |
| `--sp-font-mono` | `"JetBrains Mono", "Fira Code", "Cascadia Code", ui-monospace, monospace` | Code blocks, inline code, terminal |

### Type Scale

| Token | `rem` | `px` (16-base) | Usage |
|---|---|---|---|
| `--sp-text-xs` | `0.75rem` | 12 | Labels, badges, captions |
| `--sp-text-sm` | `0.875rem` | 14 | Sidebar nav, meta |
| `--sp-text-base` | `1rem` | 16 | Body prose |
| `--sp-text-lg` | `1.125rem` | 18 | Lead paragraph |
| `--sp-text-xl` | `1.25rem` | 20 | Sub-heading (h4) |
| `--sp-text-2xl` | `1.5rem` | 24 | Section heading (h3) |
| `--sp-text-3xl` | `1.875rem` | 30 | Page heading (h2) |
| `--sp-text-4xl` | `2.25rem` | 36 | Article title (h1) |
| `--sp-text-hero` | `3.5rem` | 56 | Landing hero headline |

### Font Weights

| Token | Value | Usage |
|---|---|---|
| `--sp-weight-normal` | `400` | Body text |
| `--sp-weight-medium` | `500` | Navigation, labels |
| `--sp-weight-semibold` | `600` | Sub-headings, button labels |
| `--sp-weight-bold` | `700` | Page headings, hero text |

### Line Heights

| Token | Value | Usage |
|---|---|---|
| `--sp-leading-tight` | `1.25` | Headings |
| `--sp-leading-snug` | `1.375` | Sub-headings |
| `--sp-leading-normal` | `1.5` | Body prose |
| `--sp-leading-relaxed` | `1.625` | Long-form text |

---

## Spacing

Uses a 4 px base grid (`--sp-space-1 = 4px`).

| Token | `rem` | `px` | Usage |
|---|---|---|---|
| `--sp-space-1` | `0.25rem` | 4 | Icon padding, micro gaps |
| `--sp-space-2` | `0.5rem` | 8 | Inline element gap |
| `--sp-space-3` | `0.75rem` | 12 | Small component padding |
| `--sp-space-4` | `1rem` | 16 | Standard padding |
| `--sp-space-5` | `1.25rem` | 20 | Medium gap |
| `--sp-space-6` | `1.5rem` | 24 | Section padding |
| `--sp-space-8` | `2rem` | 32 | Large section gap |
| `--sp-space-10` | `2.5rem` | 40 | XL gap |
| `--sp-space-12` | `3rem` | 48 | 2XL gap |
| `--sp-space-16` | `4rem` | 64 | Section spacing |
| `--sp-space-20` | `5rem` | 80 | Hero padding |
| `--sp-space-24` | `6rem` | 96 | Max hero padding |

---

## Border Radius

| Token | Value | Usage |
|---|---|---|
| `--sp-radius-sm` | `0.25rem` | Badges, tags, small chips |
| `--sp-radius-md` | `0.375rem` | Buttons, inputs, inline code |
| `--sp-radius-lg` | `0.5rem` | Cards, panels, callouts |
| `--sp-radius-xl` | `0.75rem` | Feature tiles, large cards |
| `--sp-radius-2xl` | `1rem` | Hero card, modal |
| `--sp-radius-full` | `9999px` | Pills, avatar borders |

---

## Shadows

| Token | Value | Usage |
|---|---|---|
| `--sp-shadow-sm` | `0 1px 2px 0 rgb(0 0 0 / 0.3)` | Subtle lift |
| `--sp-shadow-md` | `0 4px 6px -1px rgb(0 0 0 / 0.4), 0 2px 4px -2px rgb(0 0 0 / 0.3)` | Cards |
| `--sp-shadow-lg` | `0 10px 15px -3px rgb(0 0 0 / 0.5), 0 4px 6px -4px rgb(0 0 0 / 0.4)` | Modals, dropdowns |
| `--sp-shadow-brand` | `0 0 0 3px rgb(124 58 237 / 0.35)` | Focus rings (brand) |
| `--sp-shadow-inset` | `inset 0 2px 4px 0 rgb(0 0 0 / 0.3)` | Pressed states, inputs |

---

## Transitions

| Token | Value | Usage |
|---|---|---|
| `--sp-duration-fast` | `100ms` | Hover colour changes |
| `--sp-duration-base` | `150ms` | Most UI transitions |
| `--sp-duration-slow` | `300ms` | Panel slide, accordion |
| `--sp-ease` | `cubic-bezier(0.4, 0, 0.2, 1)` | Standard easing |
| `--sp-ease-in` | `cubic-bezier(0.4, 0, 1, 1)` | Exit animations |
| `--sp-ease-out` | `cubic-bezier(0, 0, 0.2, 1)` | Enter animations |

---

## Z-Index Scale

| Token | Value | Usage |
|---|---|---|
| `--sp-z-base` | `0` | Default stacking |
| `--sp-z-raised` | `10` | Cards on hover |
| `--sp-z-dropdown` | `100` | Dropdown menus |
| `--sp-z-sticky` | `200` | Sticky header/sidebar |
| `--sp-z-overlay` | `300` | Modal backdrops |
| `--sp-z-modal` | `400` | Modal dialogs |
| `--sp-z-toast` | `500` | Toasts / notifications |

---

## Breakpoints

| Token | Value | Usage |
|---|---|---|
| `--sp-bp-sm` | `640px` | Small phones |
| `--sp-bp-md` | `768px` | Tablets |
| `--sp-bp-lg` | `1024px` | Laptops |
| `--sp-bp-xl` | `1280px` | Desktops |
| `--sp-bp-2xl` | `1536px` | Wide screens |

---

## Docusaurus CSS Variable Mapping

The Docusaurus theme variables are mapped from tokens in `docs/site/src/css/custom.css`:

| Docusaurus var | Maps to token |
|---|---|
| `--ifm-color-primary` | `--sp-brand-600` |
| `--ifm-color-primary-dark` | `--sp-brand-600` (darkened 10%) |
| `--ifm-color-primary-light` | `--sp-brand-500` |
| `--ifm-color-primary-lighter` | `--sp-brand-400` |
| `--ifm-color-primary-lightest` | `--sp-brand-300` |
| `--ifm-font-family-base` | `--sp-font-sans` |
| `--ifm-font-family-monospace` | `--sp-font-mono` |
| `--ifm-code-font-size` | `0.875em` |
| `--ifm-background-color` | `--sp-bg` (dark) / `--sp-bg-light` (light) |
| `--ifm-navbar-background-color` | `--sp-surface` |
| `--ifm-sidebar-background` | `--sp-surface` |
