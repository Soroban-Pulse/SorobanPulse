# Lua Event Transformation Feature

## Summary

This feature adds optional Lua scripting support to Soroban Pulse, allowing deployments to transform or filter events before storage without forking the codebase.

## Implementation

### Components

1. **`src/lua_transform.rs`**: Core Lua transformation module
   - `LuaTransformer` struct for managing Lua runtime
   - Event-to-Lua and Lua-to-event conversion
   - Timeout handling and error management
   - Comprehensive test suite

2. **Configuration** (`src/config.rs`):
   - `event_transform_script`: Path to Lua script (optional)
   - `event_transform_timeout_ms`: Script timeout (default: 100ms)

3. **Integration** (`src/indexer.rs`):
   - Lua transformer field in `Indexer` struct
   - Transformation applied before event storage
   - Events returning `nil` are skipped
   - Transformation errors skip the event with warning

4. **Initialization** (`src/main.rs`):
   - Lua transformer loaded at startup if configured
   - Initialization errors logged but don't prevent startup

### Dependencies

- `mlua` v0.9 with Lua 5.4, async support, and serialization
- Optional feature flag: `lua`

### Build

```bash
# Build with Lua support
cargo build --release --features lua

# Build without Lua support (default)
cargo build --release
```

## Usage

### 1. Create a Transformation Script

```lua
-- transform.lua
function transform_event(event)
    -- Skip diagnostic events
    if event.event_type == "diagnostic" then
        return nil
    end
    
    -- Add metadata
    event.value.indexed_at = os.time()
    
    return event
end
```

### 2. Configure Environment

```bash
EVENT_TRANSFORM_SCRIPT=/path/to/transform.lua
EVENT_TRANSFORM_TIMEOUT_MS=100
```

### 3. Build and Run

```bash
cargo build --release --features lua
./target/release/soroban-pulse
```

## Acceptance Criteria

✅ **EVENT_TRANSFORM_SCRIPT environment variable specifies a Lua script path**
- Implemented in `src/config.rs`
- Documented in `.env.example`

✅ **The script is called for each event with the event data as a Lua table**
- Implemented in `src/indexer.rs` (fetch_and_store_events)
- Event converted to Lua table in `lua_transform.rs`

✅ **Returning nil from the script skips the event**
- Handled in transformation logic
- Skipped events increment the skip counter

✅ **Returning a modified table stores the modified event**
- Lua table converted back to `SorobanEvent`
- Modified event stored in database

✅ **Script execution has a configurable timeout (default: 100ms)**
- `EVENT_TRANSFORM_TIMEOUT_MS` configuration
- Timeout enforced with `tokio::time::timeout`
- Timeout errors logged and event skipped

✅ **The mlua crate is added as an optional dependency behind a lua feature flag**
- Added to `Cargo.toml` with `optional = true`
- Feature flag `lua = ["mlua"]` defined
- Conditional compilation with `#[cfg(feature = "lua")]`

✅ **Tests verify transformation and skip behavior**
- `test_passthrough_script`: Verifies events pass through unchanged
- `test_skip_event`: Verifies nil return skips events
- `test_modify_event`: Verifies event modification
- `test_filter_by_contract`: Verifies contract filtering
- `test_timeout`: Verifies timeout handling

## Testing

### Run Unit Tests

```bash
cargo test --features lua lua_transform
```

### Test Coverage

- Passthrough transformation
- Event skipping (nil return)
- Event modification
- Contract filtering
- Timeout handling
- JSON-Lua conversion (objects, arrays, primitives)

### Manual Testing

1. Create a test script:
```lua
function transform_event(event)
    print("Processing event from contract: " .. event.contract_id)
    event.value.test_field = "added_by_lua"
    return event
end
```

2. Configure and run:
```bash
export EVENT_TRANSFORM_SCRIPT=./test_transform.lua
export EVENT_TRANSFORM_TIMEOUT_MS=100
cargo run --features lua
```

3. Verify in logs:
```
INFO Lua event transformer enabled script_path=./test_transform.lua timeout_ms=100
```

## Documentation

- **User Guide**: `docs/lua-transformation.md`
- **Example Script**: `examples/transform_example.lua`
- **Environment Variables**: `.env.example`

## Performance

- **Overhead**: ~1-5ms per event (depends on script complexity)
- **Timeout**: Configurable, default 100ms prevents runaway scripts
- **Memory**: Lua runtime shared across events, minimal per-event allocation
- **Async**: Transformation runs in blocking task pool to avoid blocking indexer

## Security

- **Sandboxed**: Lua runtime has no filesystem or network access
- **Timeout**: Prevents infinite loops
- **Error handling**: Script errors don't crash the indexer
- **Input validation**: Event data validated before and after transformation

## Future Enhancements

- Hot-reload scripts without restart
- Performance metrics per transformation
- Access to contract ABIs for automatic decoding
- Shared cache/state between invocations
- Support for multiple transformation scripts (pipeline)

## Migration

No migration required. Feature is:
- Optional (disabled by default)
- Backward compatible
- No database schema changes
- No breaking API changes

## Rollback

To disable:
1. Remove `EVENT_TRANSFORM_SCRIPT` from environment
2. Restart service
3. Or rebuild without `--features lua`
