# Lua Event Transformation

Soroban Pulse supports optional Lua scripting for transforming or filtering events before they are stored in the database. This feature allows deployments to customize event processing without forking the codebase.

## Overview

The Lua transformation feature enables you to:
- Filter out unwanted events by returning `nil`
- Modify event data (decode XDR, add labels, normalize structure)
- Enrich events with additional metadata
- Apply deployment-specific business logic

## Configuration

### Environment Variables

- `EVENT_TRANSFORM_SCRIPT`: Path to the Lua script file (optional)
- `EVENT_TRANSFORM_TIMEOUT_MS`: Script execution timeout in milliseconds (default: 100)

### Example Configuration

```bash
# Enable Lua transformation
EVENT_TRANSFORM_SCRIPT=/path/to/transform.lua
EVENT_TRANSFORM_TIMEOUT_MS=100
```

### Building with Lua Support

The Lua feature is optional and must be enabled at compile time:

```bash
cargo build --release --features lua
```

## Script Structure

Your Lua script must define a `transform_event` function that receives an event table and returns either:
- A modified event table to store the event
- `nil` to skip the event entirely

### Event Schema

The event table passed to your script contains:

```lua
{
    contract_id = "CAAAA...",           -- Contract address (string)
    event_type = "contract",            -- Event type (string)
    tx_hash = "abc123...",              -- Transaction hash (string)
    ledger = 12345,                     -- Ledger number (number)
    ledger_closed_at = "2024-01-01...", -- ISO 8601 timestamp (string)
    ledger_hash = "hash123...",         -- Ledger hash (string, optional)
    in_successful_call = true,          -- Success flag (boolean)
    value = { ... },                    -- Event data (table/JSON object)
    topic = { ... }                     -- Event topics (table/array, optional)
}
```

### Basic Example

```lua
function transform_event(event)
    -- Skip diagnostic events
    if event.event_type == "diagnostic" then
        return nil
    end
    
    -- Add timestamp to all events
    event.value.processed_at = os.time()
    
    return event
end
```

## Use Cases

### 1. Filter Events by Contract

```lua
function transform_event(event)
    local allowed_contracts = {
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
        "CBEXAMPLECONTRACT123",
    }
    
    for _, contract in ipairs(allowed_contracts) do
        if event.contract_id == contract then
            return event
        end
    end
    
    return nil  -- Skip events from other contracts
end
```

### 2. Add Human-Readable Labels

```lua
function transform_event(event)
    local contract_names = {
        ["CAAAA..."] = "DEX Contract",
        ["CBBBB..."] = "Lending Pool",
    }
    
    if contract_names[event.contract_id] then
        event.value.contract_name = contract_names[event.contract_id]
    end
    
    return event
end
```

### 3. Decode or Transform Values

```lua
function transform_event(event)
    -- Convert stroops to XLM
    if event.value.amount then
        event.value.amount_xlm = event.value.amount / 10000000
    end
    
    -- Decode base64 data
    if event.value.encoded_data then
        -- Add your decoding logic here
        event.value.decoded_data = decode_base64(event.value.encoded_data)
    end
    
    return event
end
```

### 4. Filter by Topic

```lua
function transform_event(event)
    if event.topic and #event.topic > 0 then
        local first_topic = event.topic[1]
        
        -- Skip internal events
        if type(first_topic) == "string" and first_topic == "internal" then
            return nil
        end
    end
    
    return event
end
```

### 5. Normalize Event Structure

```lua
function transform_event(event)
    -- Flatten nested structures
    if event.value.data then
        for key, value in pairs(event.value.data) do
            event.value[key] = value
        end
        event.value.data = nil
    end
    
    return event
end
```

## Performance Considerations

### Timeout

Scripts that exceed `EVENT_TRANSFORM_TIMEOUT_MS` are terminated and the event is skipped. Keep transformations lightweight:

- Avoid expensive computations
- Don't make network calls
- Minimize string operations on large data

### Memory

The Lua runtime is shared across all event transformations. Avoid:
- Creating large temporary tables
- Storing global state between invocations
- Memory leaks from circular references

### Best Practices

1. **Keep it simple**: Complex logic should be in the application, not the transformation script
2. **Test thoroughly**: Use the provided tests as examples
3. **Handle errors**: The script will skip events on errors, so validate inputs
4. **Log sparingly**: Excessive logging from Lua can impact performance
5. **Version control**: Keep your transformation scripts in version control

## Testing

### Unit Tests

The `lua_transform` module includes comprehensive tests. Run them with:

```bash
cargo test --features lua lua_transform
```

### Integration Testing

Test your script with sample events:

```lua
-- test_transform.lua
function transform_event(event)
    -- Your transformation logic
    return event
end

-- Test with sample data
local test_event = {
    contract_id = "CTEST...",
    event_type = "contract",
    tx_hash = "test123",
    ledger = 100,
    ledger_closed_at = "2024-01-01T00:00:00Z",
    in_successful_call = true,
    value = { amount = 1000 },
    topic = { "transfer" }
}

local result = transform_event(test_event)
assert(result ~= nil, "Event should not be skipped")
assert(result.value.amount == 1000, "Amount should be preserved")
```

## Troubleshooting

### Script Not Loading

Check the logs for initialization errors:
```
ERROR Failed to initialize Lua transformer — transformation disabled
```

Verify:
- The script file exists and is readable
- The script has valid Lua syntax
- The `transform_event` function is defined

### Events Being Skipped

If events are unexpectedly skipped, check:
- Script timeout (increase `EVENT_TRANSFORM_TIMEOUT_MS`)
- Script errors (check logs for "Lua transformation failed")
- Logic errors (script returning `nil` unintentionally)

### Performance Issues

If the indexer is slow with Lua enabled:
- Profile your script execution time
- Reduce complexity in the transformation logic
- Consider filtering at the database level instead

## Security Considerations

1. **Sandboxing**: The Lua runtime is sandboxed and cannot access the filesystem or network
2. **Resource limits**: Scripts are subject to timeout and memory limits
3. **Input validation**: Always validate event data before processing
4. **Code review**: Review transformation scripts as you would application code

## Example Scripts

See `examples/transform_example.lua` for a comprehensive example demonstrating:
- Contract filtering
- Event type filtering
- Metadata enrichment
- Value transformation
- Topic-based filtering
- Structure normalization

## Limitations

- No access to external resources (filesystem, network, database)
- No persistent state between invocations
- Limited to synchronous operations
- Cannot modify the event schema (add/remove top-level fields)

## Future Enhancements

Potential future improvements:
- Support for async operations
- Access to contract ABIs for automatic decoding
- Shared state/cache between invocations
- Performance metrics per script
- Hot-reloading of scripts without restart
