# API Contract Testing with Pact (Issue #556)

## Overview

Contract testing ensures that API clients and servers implement the same contract. This prevents integration failures where the server and clients have incompatible expectations about request/response formats.

## What is Contract Testing?

Contract testing verifies that:
- API responses have the expected structure
- Request/response fields have the correct types
- Error responses are consistent
- HTTP status codes are appropriate
- Headers are present and formatted correctly

### Without Contract Testing (Problems)

```
Server says: Returns array    → Client expects: Object
Server says: field is number  → Client expects: string
Server says: 200 OK           → Client expects: 201 Created
```

### With Contract Testing (Safe)

Both server and client tests verify the shared contract, preventing mismatches.

## Framework: Pact

Pact is a consumer-driven contract testing framework that:

- **Captures contracts** from client expectations
- **Verifies contracts** on the server side
- **Documents APIs** through tests
- **Prevents integration surprises** through CI/CD

## Testing Scope

Our contract tests cover the following SorobanPulse API endpoints:

| Endpoint | Method | Purpose | Tests |
|----------|--------|---------|-------|
| `/events` | GET | List events | Response structure, pagination, filtering |
| `/events/{id}` | GET | Get event details | Response format, error handling |
| `/subscriptions` | POST | Create subscription | Request validation, response format |
| `/subscriptions` | GET | List subscriptions | Pagination, filtering |
| `/subscriptions/{id}` | DELETE | Delete subscription | Success/error responses |
| `/webhooks` | POST | Create webhook | Request structure, validation |
| `/webhooks/{id}/test` | POST | Test webhook | Request format, response status |
| `/health` | GET | Health check | Status response format |

### A second, broader suite: `tests/pact_contract_tests.rs`

`tests/contract_tests.rs` above is the original issue #556 suite. A second,
more extensive contract suite lives in
[`tests/pact_contract_tests.rs`](../tests/pact_contract_tests.rs) (21 tests
across 6 modules) and covers endpoints and response shapes the first suite
doesn't:

| Module | Coverage |
|---|---|
| `events_api_contracts` | `GET /v1/events` (success, filters, streaming), `GET /v1/events/contract/{id}`, `GET /v1/events/tx/{hash}` |
| `error_response_contracts` | 400, 401, 403, 404, 429, 500, 503 response shapes |
| `health_check_contracts` | Liveness probe, readiness probe (ready and DB-down states) |
| `subscription_api_contracts` | Create/list/delete subscriptions |
| `api_versioning_contracts` | Versioned endpoint paths, deprecation headers |
| `ndjson_response_contracts` | Streaming NDJSON response format |

Run it the same way as the primary suite:

```bash
cargo test --test pact_contract_tests
```

It is not currently wired into `.github/workflows/contract-testing.yml` (only
`contract_tests` runs there) — see the [Testing Checklist](#testing-checklist)
below.

## Running Contract Tests

### Run all contract tests

```bash
cargo test --test contract_tests
```

### Run specific test category

```bash
cargo test --test contract_tests contract_tests::
cargo test --test contract_tests provider_state_tests::
cargo test --test contract_tests backward_compatibility_tests::
```

### Run with output

```bash
cargo test --test contract_tests -- --nocapture
```

## Contract Test Categories

### 1. Response Structure Tests

**What they verify:**
- Response has correct JSON structure
- Required fields are present
- Fields have correct types
- Nested objects and arrays are valid

**Example**:
```rust
#[test]
fn contract_get_events_response_structure() {
    let response = json!({
        "success": true,
        "data": [ /* events */ ],
        "pagination": { /* pagination */ }
    });

    assert!(response["data"].is_array());
    assert!(response["pagination"].is_object());
}
```

**Typical issues caught:**
- Missing fields
- Type mismatches (number vs string)
- Incorrect nesting
- Malformed JSON

### 2. Request/Response Matching Tests

**What they verify:**
- Request fields match response echoes
- Response includes all request data
- Field transformations are consistent

**Example**:
```rust
#[test]
fn contract_create_subscription_request_response() {
    let request = json!({
        "contract_id": "C...",
        "webhook_url": "https://..."
    });

    let response = json!({
        "contract_id": "C...",  // Must match request
        "webhook_url": "https://..."  // Must match request
    });

    assert_eq!(request["contract_id"], response["contract_id"]);
}
```

**Typical issues caught:**
- Response doesn't echo request
- Field value transformations
- Missing response fields

### 3. Error Response Tests

**What they verify:**
- Error responses have consistent structure
- Error codes are meaningful
- Error messages are present
- Detailed error information is available

**Example**:
```rust
#[test]
fn contract_error_response_format() {
    let error = json!({
        "success": false,
        "error": {
            "code": "BAD_REQUEST",
            "message": "Invalid input",
            "details": { }
        }
    });

    assert_eq!(error["success"], false);
    assert!(error["error"]["code"].is_string());
}
```

**Typical issues caught:**
- Inconsistent error structures
- Missing error codes
- Unclear error messages
- Missing error details

### 4. Pagination Tests

**What they verify:**
- Pagination parameters are validated
- Page numbers are positive
- Limits are within bounds
- Pagination metadata is consistent

**Example**:
```rust
#[test]
fn contract_pagination_parameters() {
    let params = "page=1&limit=50";
    let page: i64 = 1;
    let limit: i64 = 50;

    assert!(page > 0);
    assert!(limit > 0 && limit <= 10_000);
}
```

**Typical issues caught:**
- Negative page numbers
- Excessive limits
- Inconsistent page counting (0-based vs 1-based)
- Missing pagination metadata

### 5. Data Format Tests

**What they verify:**
- Timestamps use consistent format (ISO 8601)
- IDs are formatted consistently
- URLs are properly encoded
- UUIDs are valid

**Example**:
```rust
#[test]
fn contract_timestamp_format_iso8601() {
    let timestamp = "2024-01-01T12:00:00Z";
    let parsed = DateTime::parse_from_rfc3339(timestamp);
    assert!(parsed.is_ok());
}
```

**Typical issues caught:**
- Timestamp format variations
- Inconsistent ID formats
- Unencoded special characters
- Invalid UUID formats

### 6. Authentication & Authorization Tests

**What they verify:**
- Authentication headers are present
- Token formats are correct
- Authorization checks work
- Permissions are enforced

**Example**:
```rust
#[test]
fn contract_authentication_header() {
    let header = "Authorization: Bearer token123";
    assert!(header.contains("Bearer"));
}
```

**Typical issues caught:**
- Missing auth headers
- Incorrect token format
- Broken permission checks
- Missing scope validation

### 7. HTTP Header Tests

**What they verify:**
- Content-Type is correct
- CORS headers are present
- Rate limit headers exist
- Cache headers are appropriate

**Example**:
```rust
#[test]
fn contract_content_type_json() {
    let ct = "application/json; charset=utf-8";
    assert!(ct.contains("application/json"));
}
```

**Typical issues caught:**
- Wrong Content-Type
- Missing CORS headers
- Broken rate limiting
- Incorrect caching directives

## Provider State Tests

Provider states define the conditions under which contract tests run.

### Example Provider States

```
State: event_exists
├─ Setup: Create event with ID 123
├─ Test: GET /events/123 returns 200
└─ Cleanup: Delete event

State: subscription_active
├─ Setup: Create active subscription
├─ Test: GET /subscriptions/{id} returns active
└─ Cleanup: Delete subscription

State: multiple_events_exist
├─ Setup: Create 100 events
├─ Test: GET /events returns paginated results
└─ Cleanup: Delete events
```

### Implementing Provider States

```rust
#[test]
fn provider_state_event_exists() {
    let provider_state = json!({
        "state_name": "event_exists",
        "event_id": "evt_123",
    });

    assert!(provider_state["state_name"].is_string());
}
```

## Backward Compatibility Tests

These tests ensure new API versions don't break existing clients.

### What They Verify

1. **Old field names still work**
2. **Old response format still valid**
3. **New fields are optional**
4. **Field types don't change**

### Example

```rust
#[test]
fn contract_backward_compatibility_v1() {
    // v1 response format
    let v1_response = json!({
        "data": [ /* ... */ ],
        "page": 1,
        "limit": 50
    });

    // Should still be valid
    assert!(v1_response["data"].is_array());
}
```

## Integration with CI/CD

### GitHub Actions Configuration

```yaml
name: Contract Tests

on: [push, pull_request]

jobs:
  contract-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      
      - name: Run contract tests
        run: cargo test --test contract_tests
      
      - name: Upload contract pacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: pact-files
          path: target/pacts/
```

### Pact Broker Integration

```yaml
- name: Publish pacts to broker
  if: github.event_name == 'push'
  run: |
    pact-broker publish \
      target/pacts \
      --consumer-app-version=${{ github.sha }} \
      --broker-base-url=${{ secrets.PACT_BROKER_URL }} \
      --broker-token=${{ secrets.PACT_BROKER_TOKEN }}
```

## Best Practices

### ✅ Do's

- **Test the contract, not the implementation**: Focus on what the API promises
- **Keep tests focused**: One test = one contract aspect
- **Use realistic data**: Test with actual expected values
- **Document variations**: Note if different versions have different contracts
- **Test error paths**: Include tests for error responses
- **Automate verification**: Run in CI on every change

### ❌ Don'ts

- **Don't hardcode dates**: Use relative comparisons instead
- **Don't test implementation details**: Implementation can change
- **Don't ignore failures**: Contract changes require coordination
- **Don't test external services**: Mock them instead
- **Don't make tests brittle**: Avoid fragile assertions
- **Don't skip error cases**: Errors are part of the contract

## Common Issues and Solutions

### Issue: Timestamp Mismatches

**Problem**: Server and client disagree on timestamp format
**Solution**: Enforce ISO 8601 in contract tests
```rust
let ts = "2024-01-01T12:00:00Z";  // Always ISO 8601
```

### Issue: Null vs Missing Fields

**Problem**: Some responses have `field: null`, others omit it
**Solution**: Document in contract which fields can be null
```rust
// Contract specifies: these fields MAY be null
// These fields MUST NOT be missing
```

### Issue: Number Type Variations

**Problem**: `limit: 50` vs `limit: "50"`
**Solution**: Enforce number types in contract tests
```rust
assert!(field.is_number());  // Not string
```

### Issue: Inconsistent Error Codes

**Problem**: Same error returned with different codes
**Solution**: Document standard error codes in contract
```
BAD_REQUEST - input validation failed
NOT_FOUND - resource doesn't exist
CONFLICT - resource already exists
```

## Testing Checklist

Before merging API changes:

- [ ] All contract tests pass (`cargo test --test contract_tests` and `cargo test --test pact_contract_tests`)
- [ ] Provider states are documented
- [ ] Error responses are consistent
- [ ] Backward compatibility is maintained
- [ ] New fields are optional (for v1 compatibility)
- [ ] Documentation is updated
- [ ] Contract pacts are published to broker
- [ ] Consumer integration tests pass

## Tools and References

- **Pact Documentation**: https://docs.pact.foundation/
- **Pact Rust**: https://github.com/pact-foundation/pact-rust
- **Pact Broker**: https://docs.pact.foundation/pact_broker/overview
- **Consumer-Driven Contracts**: https://martinfowler.com/articles/consumerDrivenContracts.html

## Project Configuration

- **Test file**: `tests/contract_tests.rs`
- **Configuration**: Cargo.toml with pact dependency
- **Documentation**: This file
- **Issue**: #556
- **Assignee**: @Xoulomon

## Future Enhancements

1. **Pact Broker Integration**: Publish contracts for coordination
2. **OpenAPI Validation**: Validate against OpenAPI specification
3. **Performance Contracts**: Ensure response times meet SLA
4. **Chaos Engineering**: Test with invalid/missing fields
5. **Consumer Validation**: Verify consumer implementations
6. **Contract Evolution**: Document breaking changes

## Questions?

For questions about contract testing, refer to:
1. Contract test file: `tests/contract_tests.rs`
2. Pact documentation: https://docs.pact.foundation/
3. GitHub issue: #556
