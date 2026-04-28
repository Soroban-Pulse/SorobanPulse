-- Simple event filter example
-- This script demonstrates basic filtering and transformation

function transform_event(event)
    -- Skip all diagnostic events
    if event.event_type == "diagnostic" then
        return nil
    end
    
    -- Skip events from a specific contract
    if event.contract_id == "CBLOCKED123" then
        return nil
    end
    
    -- Add a timestamp to all events
    if event.value then
        event.value.lua_processed_at = os.time()
    end
    
    -- Pass through all other events
    return event
end
