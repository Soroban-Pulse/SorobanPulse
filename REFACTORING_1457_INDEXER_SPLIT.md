# Issue #1457 - Split src/indexer.rs into RPC, Persistence, and Leader Election

## Problem

Indexer (2,088 lines) mixes 5 distinct concerns:
- Advisory lock leader election (416–476)
- RPC polling with exponential backoff
- Event normalization (cryptographic validation)
- Large INSERT INTO events (23 columns)
- Savepoint handling for rollback
- Hash-chain validation
- Tests including 3-second promotion test

## Proposed Structure

```
src/indexer/
├── mod.rs                        -- Coordinator, main loop
├── leader_election.rs            -- Advisory lock, failover
├── rpc_poller.rs                 -- Polling with backoff
├── event_normalizer.rs           -- Crypto validation, normalization
├── persistence.rs                -- Database writes, savepoints
├── hash_validator.rs             -- Hash-chain verification
└── tests.rs                       -- Integration tests
```

## Module Breakdown

| Module | Lines | Responsibility |
|--------|-------|-----------------|
| leader_election.rs | ~150 | Advisory locks, failover, heartbeat |
| rpc_poller.rs | ~400 | Poll endpoints, backoff, retry strategy |
| event_normalizer.rs | ~300 | Crypto validation, timestamp normalization |
| persistence.rs | ~500 | INSERT, savepoints, rollback, indexes |
| hash_validator.rs | ~200 | Hash-chain verification, proof validation |
| mod.rs | ~200 | Coordinator, heartbeat, error handling |
| tests.rs | ~350 | Integration tests, mock RPC |

## Implementation Order

1. Create src/indexer/ directory
2. Move leader_election logic → leader_election.rs
3. Move rpc_poller logic → rpc_poller.rs
4. Move event_normalizer logic → event_normalizer.rs
5. Move persistence logic → persistence.rs
6. Move hash_validator logic → hash_validator.rs
7. Update mod.rs coordinator
8. Move tests → tests.rs

## Expected Outcome

**Before:** indexer.rs (2,088 lines)  
**After:** 7 focused modules (~300 lines each) + mod.rs (~200 lines)

## Benefits

✅ Clear separation: leader election vs persistence vs validation  
✅ Easier to test each component independently  
✅ Failover logic isolated and testable  
✅ Database layer decoupled from RPC  
✅ Crypto validation is focused  
✅ Easier to optimize persistence separately  

## Checklist

- [ ] Design module structure
- [ ] Create src/indexer/ directory
- [ ] Move leader_election logic
- [ ] Move rpc_poller logic
- [ ] Move event_normalizer logic
- [ ] Move persistence logic
- [ ] Move hash_validator logic
- [ ] Update mod.rs coordinator
- [ ] Move integration tests
- [ ] Update imports across codebase
- [ ] Add module documentation
