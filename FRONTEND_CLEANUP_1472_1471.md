# Issues #1472 & #1471 - Frontend Quality: A11y & API Type Sync

## Issue #1472 - Automated Accessibility (a11y) Checks

### Problem

No automated a11y testing despite accessibility guide. Common failures:
- Data-dense tables without proper ARIA
- ScVal trees lacking focus management
- Live-updating streams without aria-live regions
- Copy buttons without accessible labels
- Colour contrast violations

**Current:** docs/accessibility-guide.md exists, but not enforced in CI

### Implementation Strategy

#### Phase 1 - Add axe-core to Playwright (PR 1)

**Install dependency:**
```bash
npm install --save-dev @axe-core/playwright
```

**Create web/e2e/accessibility.spec.ts:**

```typescript
import { test, expect } from '@playwright/test';
import { injectAxe, checkA11y } from '@axe-core/playwright';

test.describe('Accessibility', () => {
  test('Event Explorer - no axe violations', async ({ page }) => {
    await page.goto('http://localhost:3000/explorer/events');
    await injectAxe(page);
    await checkA11y(page, null, {
      detailedReport: true,
      detailedReportOptions: {
        html: true
      }
    }, {
      rules: {
        // Suppress false positives if needed
      }
    });
  });

  test('Event Detail - no axe violations', async ({ page }) => {
    await page.goto('http://localhost:3000/explorer/events/abc123');
    await injectAxe(page);
    await checkA11y(page, null);
  });

  test('Marketplace - no axe violations', async ({ page }) => {
    await page.goto('http://localhost:3000/marketplace');
    await injectAxe(page);
    await checkA11y(page, null);
  });

  test('Form interactions - keyboard navigable', async ({ page }) => {
    await page.goto('http://localhost:3000/explorer/events');
    
    // Tab through form
    await page.keyboard.press('Tab');
    await expect(page.locator(':focus')).toBeTruthy();
    
    // Verify focused element is visible
    const focused = await page.locator(':focus').isVisible();
    expect(focused).toBe(true);
  });

  test('Live regions - aria-live polite', async ({ page }) => {
    await page.goto('http://localhost:3000/explorer/events?stream=true');
    
    // Find streaming container
    const liveRegion = page.locator('[aria-live="polite"]');
    await expect(liveRegion).toBeTruthy();
    
    // Verify it updates (screen reader announces changes)
    const textBefore = await liveRegion.textContent();
    await page.waitForTimeout(500);
    const textAfter = await liveRegion.textContent();
    
    // Should eventually change as new events arrive
  });
});
```

#### Phase 2 - Add ESLint jsx-a11y Plugin (PR 2)

**Install:**
```bash
npm install --save-dev eslint-plugin-jsx-a11y
```

**Update web/eslint.config.js:**

```typescript
import jsxA11y from 'eslint-plugin-jsx-a11y';

export default [
  {
    files: ['src/**/*.tsx'],
    plugins: {
      'jsx-a11y': jsxA11y,
    },
    rules: {
      'jsx-a11y/alt-text': 'error',
      'jsx-a11y/aria-role': 'error',
      'jsx-a11y/aria-unsupported-elements': 'error',
      'jsx-a11y/click-events-have-key-events': 'warn',
      'jsx-a11y/no-static-element-interactions': 'warn',
      'jsx-a11y/label-has-associated-control': 'error',
      'jsx-a11y/no-redundant-roles': 'warn',
    },
  },
];
```

#### Phase 3 - Manual Keyboard & Screen Reader Testing (PR 3)

**Test Event Explorer (keyboard-only):**

1. Navigate with Tab/Shift+Tab
   - [ ] All interactive elements reachable
   - [ ] Focus visible on all elements
   - [ ] Focus order logical (top-to-bottom, left-to-right)

2. Test filters without mouse:
   - [ ] Dropdowns open/close with Enter
   - [ ] Checkboxes toggle with Space
   - [ ] Submit via Enter on form

3. Test data table:
   - [ ] Headers have scope="col"
   - [ ] Can sort columns via keyboard
   - [ ] Pagination links accessible

**Test with NVDA/JAWS (screen reader):**

1. Event list announces:
   - [ ] Table headers properly announced
   - [ ] Row count announced
   - [ ] Each row content clear

2. Buttons announce:
   - [ ] Copy button: "Copy to clipboard, button"
   - [ ] Details link: "View details for transaction ABC123"
   - [ ] No redundant ARIA

3. Live stream updates:
   - [ ] aria-live="polite" region announces new events
   - [ ] No interrupting (use polite not assertive)
   - [ ] Pause button stops announcements

**Document findings in PR description**

#### Phase 4 - Fix Common Violations (PR 4)

**Example fixes:**

```typescript
// ❌ BEFORE
<button onClick={handleCopy}>📋</button>

// ✅ AFTER
<button 
  onClick={handleCopy}
  aria-label="Copy transaction ID to clipboard"
>
  📋
</button>
```

```typescript
// ❌ BEFORE
<div onClick={handleExpand}>
  Transaction details
</div>

// ✅ AFTER
<button 
  onClick={handleExpand}
  aria-expanded={isExpanded}
  aria-controls="details-panel"
>
  Transaction details
</button>
```

```typescript
// ❌ BEFORE
<table>
  <tr><td>Header 1</td><td>Header 2</td></tr>
  <tr><td>Value 1</td><td>Value 2</td></tr>
</table>

// ✅ AFTER
<table>
  <thead>
    <tr>
      <th scope="col">Header 1</th>
      <th scope="col">Header 2</th>
    </tr>
  </thead>
  <tbody>
    <tr>
      <td>Value 1</td>
      <td>Value 2</td>
    </tr>
  </tbody>
</table>
```

#### Phase 5 - Add to CI Pipeline (PR 5)

**.github/workflows/e2e-web.yml:**

```yaml
- name: Run accessibility tests
  run: npm run test:a11y
  
- name: Lint accessibility
  run: npm run lint:a11y
```

**package.json:**

```json
{
  "scripts": {
    "test:a11y": "playwright test e2e/accessibility.spec.ts",
    "lint:a11y": "eslint src --plugin jsx-a11y"
  }
}
```

### Checklist

- [ ] Install @axe-core/playwright
- [ ] Create accessibility.spec.ts with axe tests
- [ ] Install eslint-plugin-jsx-a11y
- [ ] Update eslint.config.js with a11y rules
- [ ] Manual keyboard-only testing (Event Explorer, filters, table)
- [ ] Manual screen reader testing (NVDA/JAWS)
- [ ] Fix violations found
- [ ] Add a11y tests to CI
- [ ] Document in docs/accessibility-guide.md

---

## Issue #1471 - Enforce API Type Sync in CI

### Problem

Frontend OpenAPI contract can drift from backend:
- Two OpenAPI generators (gen_openapi.rs, dump_openapi.rs)
- Committed openapi.json can be stale
- Backend doesn't compile → can't regenerate spec
- Frontend gets runtime errors in production

### Current State

```
backend/
├── src/bin/gen_openapi.rs         (OpenAPI generator 1)
├── src/bin/dump_openapi.rs        (OpenAPI generator 2)
└── ...

tests/
└── router_smoke_openapi_allowlist.txt  (missing routes list)

web/
├── orval.config.ts                (generates from openapi.json)
├── gen-api script
└── src/api/ (generated client)

openapi.json (committed, may be stale)
```

### Implementation Strategy

#### Phase 1 - Consolidate OpenAPI Generator (PR 1)

**Audit both generators:**
```bash
# Check gen_openapi.rs
wc -l backend/src/bin/gen_openapi.rs

# Check dump_openapi.rs
wc -l backend/src/bin/dump_openapi.rs

# Determine: which is used? Which is better?
grep -r "gen_openapi\|dump_openapi" .github/
```

**Decision:** Keep one, delete other

Recommendation: Keep the simpler/cleaner one, document in README

#### Phase 2 - Add OpenAPI Regeneration to CI (PR 2)

**.github/workflows/openapi-sync.yml:**

```yaml
name: OpenAPI Sync Check
on:
  push:
    paths:
      - 'backend/src/**'
      - '.github/workflows/openapi-sync.yml'

jobs:
  regenerate-openapi:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Build backend
        run: cargo build --release --bin gen_openapi
        working-directory: backend
      
      - name: Regenerate openapi.json
        run: |
          cargo run --release --bin gen_openapi > openapi.json.new
          
      - name: Check openapi.json up to date
        run: |
          if ! diff -q openapi.json openapi.json.new > /dev/null; then
            echo "ERROR: openapi.json is out of date"
            echo "Run: cargo run --release --bin gen_openapi > openapi.json"
            diff openapi.json openapi.json.new
            exit 1
          fi
          rm openapi.json.new
      
      - name: Generate web client
        run: npm run gen-api
        working-directory: web
      
      - name: Check web client up to date
        run: |
          if ! git diff --exit-code web/src/api/; then
            echo "ERROR: web API client is out of date"
            echo "Run: npm run gen-api"
            exit 1
          fi
```

#### Phase 3 - Shrink Allowlist (PR 3)

**Current router_smoke_openapi_allowlist.txt likely has many entries**

Gradually reduce by annotating missing routes in code:

```rust
// backend/src/routes.rs

/// Health check endpoint
/// (Intentionally not in OpenAPI spec; internal only)
#[get("/health")]
async fn health() -> Json<Value> { ... }

/// Admin-only debug endpoint
/// (Not public API, excluded from OpenAPI)
#[get("/admin/debug")]
async fn debug() -> Json<Value> { ... }
```

**Update CI to verify annotations:**

```bash
# Check any missing routes have comments
routes=$(cargo run --release --bin list_routes)
allowlist=$(cat router_smoke_openapi_allowlist.txt)

for route in $routes; do
  if ! grep -q "$route" openapi.json; then
    # Verify it has a comment in code
    if ! grep -B 2 "fn.*$route" backend/src/routes.rs | \
        grep -q "Intentionally\|Not public\|internal"; then
      echo "Missing route $route must have comment"
      exit 1
    fi
  fi
done
```

Target: allowlist.txt ≤ 5 entries (or zero)

#### Phase 4 - Add Type Verification (PR 4)

**Add to CI after client regeneration:**

```bash
# Verify TypeScript compiles
npm run build
# Verify no type errors
npx tsc --noEmit

# Optional: semantic versioning check
# (Warn if API changed in breaking way)
npm run api:check-breaking
```

#### Phase 5 - Document API Contract (PR 5)

Create CONTRIBUTING.md section:

```markdown
## OpenAPI & Type Safety

### Keeping API & Frontend in Sync

If you modify backend routes, the CI will regenerate `openapi.json` and update `web/src/api/`.

**Before merging:**
1. Ensure backend compiles: `cargo build`
2. Run CI locally: `npm run ci:openapi-sync`
3. If openapi.json changed, commit it
4. If web/src/api/ changed, commit it

**To regenerate locally:**
```bash
# Backend
cargo run --release --bin gen_openapi > openapi.json

# Frontend
cd web && npm run gen-api
```

### OpenAPI Routes Allowlist

Routes not in `openapi.json` (admin/internal only) must be in:
- `router_smoke_openapi_allowlist.txt`
- **AND** have a code comment explaining why

Example:
```rust
/// Internal-only endpoint, not part of public API
#[get("/admin/debug")]
fn debug() { ... }
```
```

#### Phase 6 - Delete Duplicate Generator (PR 6)

```bash
rm backend/src/bin/dump_openapi.rs  # (or the other one)
git commit -m "Remove duplicate OpenAPI generator"
```

### Checklist

- [ ] Identify which OpenAPI generator to keep
- [ ] Add openapi-sync.yml workflow
- [ ] Test: regenerate openapi.json locally
- [ ] Test: npm run gen-api generates client
- [ ] Add router annotations for excluded routes
- [ ] Shrink router_smoke_openapi_allowlist.txt
- [ ] Add type verification to CI
- [ ] Delete duplicate generator binary
- [ ] Document in CONTRIBUTING.md

## Coordination

#1472 (a11y) and #1471 (API sync) are independent but related:
- #1471 ensures frontend types match backend contract (compile-time safety)
- #1472 ensures frontend is usable (runtime accessibility)
- Both reduce production bugs

## Benefits (Both Issues)

✅ Automated a11y checks catch regressions  
✅ Keyboard & screen reader users supported  
✅ API contract never stale  
✅ Frontend types always correct  
✅ Zero runtime type mismatches  
✅ CI enforces quality standards  
