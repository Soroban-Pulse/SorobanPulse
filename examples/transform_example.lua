-- Example Lua transformation script for Soroban Pulse
-- This script demonstrates various event transformation patterns

-- Main transformation function called for each event
-- @param event table - The event data with fields:
--   - contract_id: string
--   - event_type: string
--   - tx_hash: string
--   - ledger: number
--   - ledger_closed_at: string (ISO 8601 timestamp)
--   - ledger_hash: string (optional)
--   - in_successful_call: boolean
--   - value: table (JSON object)
--   - topic: table (array of JSON values, optional)
-- @return table|nil - Modified event to store, or nil to skip
function transform_event(event)
    -- Example 1: Skip events from specific contracts
    local blocked_contracts = {
        "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
    }
    for _, contract in ipairs(blocked_contracts) do
        if event.contract_id == contract then
            return nil  -- Skip this event
        end
    end

    -- Example 2: Filter by event type
    if event.event_type == "diagnostic" then
        return nil  -- Skip diagnostic events
    end

    -- Example 3: Add metadata to event value
    if event.value then
        event.value.indexed_at = os.time()
        event.value.ledger_number = event.ledger
    end

    -- Example 4: Decode or enrich specific contract events
    if event.contract_id == "CBEXAMPLECONTRACT123" then
        if event.value and event.value.amount then
            -- Convert amount to human-readable format
            event.value.amount_formatted = string.format("%.2f XLM", event.value.amount / 10000000)
        end
    end

    -- Example 5: Add labels to known contracts
    local contract_labels = {
        ["CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM"] = "Test Contract",
        ["CBEXAMPLECONTRACT123"] = "Payment Processor",
    }
    if contract_labels[event.contract_id] then
        event.value.contract_label = contract_labels[event.contract_id]
    end

    -- Example 6: Filter by topic
    if event.topic and #event.topic > 0 then
        local first_topic = event.topic[1]
        if type(first_topic) == "string" and first_topic == "internal" then
            return nil  -- Skip internal events
        end
    end

    -- Example 7: Normalize event data structure
    if event.value and event.value.data then
        -- Flatten nested data
        for key, value in pairs(event.value.data) do
            event.value[key] = value
        end
        event.value.data = nil
    end

    -- Return the modified event (or original if no changes)
    return event
end
