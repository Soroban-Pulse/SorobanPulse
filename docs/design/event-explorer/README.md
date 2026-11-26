# Event Explorer — Wireframes & Mockups

Design source for the Event Explorer page (`dashboard/src/pages/EventExplorerPage.tsx`).
Files are SVG so they diff in review and scale cleanly; export to PNG with
`rsvg-convert -w 1280 <file>.svg -o <file>.png` if a raster copy is needed.

| Screen | File |
|--------|------|
| Desktop layout (filter bar, event table, pagination) | [desktop-wireframe.svg](desktop-wireframe.svg) |
| Mobile layout (stacked cards, collapsible filters, load more) | [mobile-wireframe.svg](mobile-wireframe.svg) |
| Loading state (skeleton rows) | [loading-state.svg](loading-state.svg) |
| Empty state (no results for filters) | [empty-state.svg](empty-state.svg) |
| Error state (banner with retry) | [error-state.svg](error-state.svg) |

## Layout notes

- **Filter bar:** contract ID, event type, ledger/date range, Search. Stays enabled while loading.
- **Table:** ledger · contract · type · timestamp · topic preview. Row click opens the event drawer.
- **Pagination:** previous / page indicator / next on desktop; "Load more" on mobile.
- **Mobile (<640px):** sidebar collapses to a hamburger, rows become cards, filters move into a sheet.
- States use the shared patterns in [`../patterns.md`](../patterns.md).

## Sign-off

Implementation should not start until two maintainers have approved below (via PR review).

| Maintainer | Status |
|------------|--------|
| _pending_ | ☐ |
| _pending_ | ☐ |
