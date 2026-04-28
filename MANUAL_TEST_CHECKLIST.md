# Manual Test Checklist for Lua Event Transformation

## Pre-requisites
- Rust toolchain installed
- PostgreSQL running
- Soroban RPC endpoint accessible

## Test Scenarios

### 1. Build Verification

#### Test 1.1: Build without lua feature (default)
```bash
cargo build --release
```
**Expected**: ✅ Builds successfully without mlua dependency

#### Test 1.2: Build with lua feature
```bash
cargo build --release --features lua
```
**Expected**: ✅ Builds successfully with mlua dependency

#### Test 1.3: Run unit tests
```bash
cargo test --features lua lua_transform
```
**Expected**: ✅ All 6 tests pass:
- test_passthrough_script
- test_skip_event
- test_modify_event
- test_filter_by_contract
- test_timeout
- JSON conversion tests

### 2. Configuration Tests

#### Test 2.1: Run without EVENT_TRANSFORM_SCRIPT
```bash
cargo run --features lua
```
**Expected**: ✅ Starts normally, no Lua transformer initialized

#### Test 2.2: Run with invalid script path
```bash
EVENT_TRANSFORM_SCRIPT=/nonexistent/script.lua cargo run --features lua
```
**Expected**: ✅ Logs error but continues running
```
ERROR Failed to initialize Lua transformer — transformation disabled
```

#### Test 2.3: Run with valid script
```bash
EVENT_TRANSFORM_SCRIPT=./examples/simple_filter.lua cargo run --features lua
```
**Expected**: ✅ Logs success
```
INFO Lua event transformer enabled script_path=./examples/simple_filter.lua timeout_ms=100
```

### 3. Transformation Tests

#### Test 3.1: Passthrough transformation
Create `test_passthrough.lua`:
```lua
function transform_event(event)
    return event
end
```

Run with:
```bash
EVENT_TRANSFORM_SCRIPT=./test_passthrough.lua cargo run --features lua
```

**Expected**: ✅ All events stored unchanged

#### Test 3.2: Skip events
Create `test_skip.lua`:
```lua
function transform_event(event)
    if event.event_type == "diagnostic" then
        return nil
    end
    return event
end
```

**Expected**: ✅ Diagnostic events skipped, others stored

#### Test 3.3: Modify events
Create `test_modify.lua`:
```lua
function transform_event(event)
    event.value.lua_processed = true
    event.value.timestamp = os.time()
    return event
end
```

**Expected**: ✅ Events stored with additional fields

#### Test 3.4: Filter by contract
Create `test_filter.lua`:
```lua
function transform_event(event)
    local allowed = {
        "CONTRACT_ID_1",
        "CONTRACT_ID_2"
    }
    for _, id in ipairs(allowed) do
        if event.contract_id == id then
            return event
        end
    end
    return nil
end
```

**Expected**: ✅ Only events from allowed contracts stored

### 4. Error Handling Tests

#### Test 4.1: Script with syntax error
Create `test_syntax_error.lua`:
```lua
function transform_event(event)
    return event
end
-- Missing 'end'
```

**Expected**: ✅ Initialization fails with clear error message

#### Test 4.2: Script with runtime error
Create `test_runtime_error.lua`:
```lua
function transform_event(event)
    error("Intentional error")
    return event
end
```

**Expected**: ✅ Events skipped with warning logged

#### Test 4.3: Script timeout
Create `test_timeout.lua`:
```lua
function transform_event(event)
    while true do end
    return event
end
```

Run with short timeout:
```bash
EVENT_TRANSFORM_SCRIPT=./test_timeout.lua EVENT_TRANSFORM_TIMEOUT_MS=50 cargo run --features lua
```

**Expected**: ✅ Events skipped with timeout warning

#### Test 4.4: Missing transform_event function
Create `test_missing_function.lua`:
```lua
function wrong_name(event)
    return event
end
```

**Expected**: ✅ Initialization fails with clear error

### 5. Performance Tests

#### Test 5.1: Measure transformation overhead
Create `test_performance.lua`:
```lua
function transform_event(event)
    -- Simple transformation
    event.value.processed = true
    return event
end
```

**Expected**: ✅ Overhead < 5ms per event

#### Test 5.2: Complex transformation
Create `test_complex.lua`:
```lua
function transform_event(event)
    -- Complex logic
    for i = 1, 1000 do
        event.value["field_" .. i] = i
    end
    return event
end
```

**Expected**: ✅ Completes within timeout or times out gracefully

### 6. Integration Tests

#### Test 6.1: End-to-end with real events
1. Start indexer with transformation script
2. Wait for events to be indexed
3. Query database to verify transformations applied

**Expected**: ✅ Events in database match transformation logic

#### Test 6.2: SSE stream with transformation
1. Start indexer with transformation script
2. Connect to SSE endpoint
3. Verify streamed events are transformed

**Expected**: ✅ SSE events match transformation logic

#### Test 6.3: Webhook delivery with transformation
1. Configure webhook
2. Start indexer with transformation script
3. Verify webhook receives transformed events

**Expected**: ✅ Webhook payloads match transformation logic

### 7. Edge Cases

#### Test 7.1: Empty event value
```lua
function transform_event(event)
    if event.value then
        event.value.processed = true
    end
    return event
end
```

**Expected**: ✅ Handles nil/empty values gracefully

#### Test 7.2: Large event data
Create event with large value object

**Expected**: ✅ Handles large data within timeout

#### Test 7.3: Special characters in data
Create event with unicode, special chars

**Expected**: ✅ Preserves data integrity

### 8. Regression Tests

#### Test 8.1: Without lua feature flag
```bash
cargo build --release
cargo run --release
```

**Expected**: ✅ Works exactly as before, no Lua code compiled

#### Test 8.2: Existing functionality unchanged
Test all existing endpoints without Lua transformation

**Expected**: ✅ All endpoints work as before

## Verification Checklist

- [ ] Code compiles without warnings
- [ ] All unit tests pass
- [ ] No diagnostics/linting errors
- [ ] Documentation is complete
- [ ] Example scripts work
- [ ] Configuration is documented
- [ ] Error messages are clear
- [ ] Performance is acceptable
- [ ] Security considerations addressed
- [ ] Backward compatibility maintained

## Test Results Template

```
Test Date: ___________
Tester: ___________
Environment: ___________

Build Tests:
- [ ] Build without lua feature: PASS/FAIL
- [ ] Build with lua feature: PASS/FAIL
- [ ] Unit tests: PASS/FAIL (___/6 passed)

Configuration Tests:
- [ ] No script configured: PASS/FAIL
- [ ] Invalid script path: PASS/FAIL
- [ ] Valid script: PASS/FAIL

Transformation Tests:
- [ ] Passthrough: PASS/FAIL
- [ ] Skip events: PASS/FAIL
- [ ] Modify events: PASS/FAIL
- [ ] Filter by contract: PASS/FAIL

Error Handling Tests:
- [ ] Syntax error: PASS/FAIL
- [ ] Runtime error: PASS/FAIL
- [ ] Timeout: PASS/FAIL
- [ ] Missing function: PASS/FAIL

Performance Tests:
- [ ] Simple transformation overhead: ___ms
- [ ] Complex transformation: PASS/FAIL

Integration Tests:
- [ ] End-to-end: PASS/FAIL
- [ ] SSE stream: PASS/FAIL
- [ ] Webhook delivery: PASS/FAIL

Edge Cases:
- [ ] Empty values: PASS/FAIL
- [ ] Large data: PASS/FAIL
- [ ] Special characters: PASS/FAIL

Regression Tests:
- [ ] Without lua feature: PASS/FAIL
- [ ] Existing functionality: PASS/FAIL

Overall Result: PASS/FAIL
Notes: ___________
```

## Automated Test Command

Run all tests at once:
```bash
# Unit tests
cargo test --features lua lua_transform

# Integration tests (if available)
cargo test --features lua --test '*'

# All tests
cargo test --features lua --all
```

## CI/CD Integration

Add to CI pipeline:
```yaml
- name: Test Lua feature
  run: |
    cargo test --features lua lua_transform
    cargo build --features lua
    cargo clippy --features lua -- -D warnings
```
