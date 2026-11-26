# Issue #1458 - Shrink src/lib.rs Public API (166 → 2-3 pub mods)

## Problem

src/lib.rs declares 166 modules, all pub. The crate is primarily a service binary, but everything is exposed as a library API, which:
- Makes clippy::pedantic docs lints noisier
- Prevents dead-code detection (rustc never warns about unused pub items)
- Invites tight coupling from tests and binaries
- Inflates binary size with unused symbols

## Strategy

**Rule: Only pub if used outside src/**

### Modules That Stay Public (2-3)
- `error` - Used by handlers and external API consumers
- `types` - Used by external consumers
- (optionally) `config` - Used by binary initialization

### Modules That Become pub(crate) (163+)
Everything else becomes internal:
- `indexer` - Only used by binary main
- `handlers` - Only used by binary server
- `middleware` - Only used by binary server
- `rpc_client` - Only used by indexer
- `database` - Only used by services
- `migrations` - Only used by binary
- All service modules (api_key_service, etc.)
- All utility modules (cache_layer, etc.)

## Implementation Steps

1. **Audit Dependencies:**
   - Grep for external imports from each module
   - Identify which modules are actually used outside src/

2. **Mark Internal Modules:**
   - Change `pub mod` → `pub(crate) mod` for internal-only
   - Keep only 2-3 essential pub items

3. **Update Tests:**
   - Internal tests use `crate::` prefix
   - External tests import via public API only

4. **Measure Impact:**
   - Binary size reduction (expected 2-5%)
   - Compilation time (may improve dead-code detection)
   - Clippy warnings (expected 50+ fewer)

## Expected Outcomes

| Aspect | Before | After |
|--------|--------|-------|
| Public items | 166 | 2-3 |
| Dead-code detection | Disabled | Enabled |
| Clippy noise | High | Low |
| Binary size | Bloated | Trimmed |
| External coupling | Easy | Hard |
| Test clarity | Unclear imports | Clear internal access |

## Verification

```bash
# After changes:
cargo build --release
cargo clippy --all-targets -- -D warnings
cargo check --lib --release
ls -lh target/release/soroban_pulse  # Check binary size
```

## Checklist

- [ ] Audit all 166 modules for external usage
- [ ] Create pub(crate) PR with majority of modules
- [ ] Keep only essential pub items (error, types)
- [ ] Update all internal imports
- [ ] Update tests to use internal paths
- [ ] Verify zero clippy warnings
- [ ] Check binary size reduction
- [ ] Update documentation

## Coordination with Other Refactors

After #1455 (config split), #1457 (indexer split), #1456 (handlers split):
- Many modules become clearer as internal
- Public API naturally shrinks to essential items only
- Can be done as final cleanup step
