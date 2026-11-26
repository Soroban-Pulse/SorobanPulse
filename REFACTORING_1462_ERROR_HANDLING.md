# Issue #1462 - Unify Error Handling Around Single AppError

## Problem Statement

Handlers return a mix of:
- `Result<_, AppError>` (defined, RFC 7807 support)
- `Result<_, (StatusCode, Json<Value>)>` (ad-hoc)
- `json!({"error": …})` (manual construction)
- Non-existent `ApiError` type (imported but undefined)

This breaks the documented API contract (RFC 7807 application/problem+json) and causes:
- Inconsistent error shapes (error vs detail field)
- Inconsistent status codes (500 vs 400 for server errors)
- Client confusion with unpredictable error format
- Violations of OpenAPI spec

## Current State

**src/error.rs (1,077 lines):**
- Defines AppError with RFC 7807 problem-details support
- Has IntoResponse impl for consistent JSON output
- Supports multiple error variants

**Problem: Many handlers bypass AppError:**
```rust
// ❌ BAD: src/handlers/mod.rs:1649 (refresh_statistics)
Result<(), (StatusCode, Json<Value>)>

// ❌ BAD: src/batch_operations.rs:341
json!({"error": "..."})

// ❌ BAD: src/api_key_service.rs (imports non-existent ApiError)
use crate::error::ApiError;
```

## Proposed Unified Error Structure

### Error Variants (RFC 7807)

```rust
// src/error.rs

#[derive(Debug, Clone)]
pub enum AppError {
    // 4xx Client Errors
    BadRequest(String),
    Unauthorized,
    Forbidden(String),
    NotFound(String),
    Conflict(String),
    Unprocessable(Vec<String>), // 422 validation errors
    TooManyRequests,
    PayloadTooLarge(String),
    
    // 5xx Server Errors
    InternalServerError(String),
    NotImplemented(String),
    ServiceUnavailable(String),
    
    // Database
    DatabaseError(String),
    DatabaseConnError(String),
    
    // Authentication
    InvalidToken(String),
    ExpiredToken,
    
    // Validation
    ValidationError(Vec<String>),
    
    // External service
    ExternalServiceError { service: String, reason: String },
    
    // Rate limiting
    RateLimitExceeded { retry_after: Option<u64> },
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, type_uri, detail) = match self {
            Self::BadRequest(msg) => (
                StatusCode::BAD_REQUEST,
                "https://api.sorobanpulse.dev/errors/bad-request",
                msg,
            ),
            Self::Unauthorized => (
                StatusCode::UNAUTHORIZED,
                "https://api.sorobanpulse.dev/errors/unauthorized",
                "Authentication required".to_string(),
            ),
            Self::NotFound(msg) => (
                StatusCode::NOT_FOUND,
                "https://api.sorobanpulse.dev/errors/not-found",
                msg,
            ),
            Self::Conflict(msg) => (
                StatusCode::CONFLICT,
                "https://api.sorobanpulse.dev/errors/conflict",
                msg,
            ),
            // ... etc
        };

        // RFC 7807 response
        (status, Json(json!({
            "type": type_uri,
            "status": status.as_u16(),
            "detail": detail,
            "instance": "/path/to/resource",
        }))).into_response()
    }
}
```

### Implementation Steps

#### Phase 1 - Add Missing Variants (PR 1)
Add to AppError:
- Conflict (409)
- PayloadTooLarge (413)
- ServiceUnavailable (503)
- NotImplemented (501)
- Unprocessable (422)

#### Phase 2 - Migrate Handlers (PR 2)
1. Audit all 50+ handlers
2. Find all non-AppError returns
3. Convert `(StatusCode, Json<Value>)` → `AppError`
4. Convert raw `json!({...})` → `AppError`

Example migrations:
```rust
// BEFORE
pub async fn refresh_statistics() -> Result<(), (StatusCode, Json<Value>)> {
    Err((
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({"error": "DB unavailable"}))
    ))
}

// AFTER
pub async fn refresh_statistics() -> Result<(), AppError> {
    Err(AppError::ServiceUnavailable(
        "Database temporarily unavailable".to_string()
    ))
}
```

#### Phase 3 - Remove ApiError (PR 3)
1. Find all `use crate::error::ApiError` imports
2. Replace with `use crate::error::AppError`
3. Remove ApiError type alias definition
4. Verify compilation

#### Phase 4 - Add Status Code Tests (PR 4)
Create contract test that validates every endpoint:

```rust
#[test]
async fn all_endpoints_return_valid_problem_json() {
    let invalid_cases = vec![
        ("GET /api/events", json!({})),  // Missing required fields
        ("POST /api/transfers", json!({
            "amount": -100,  // Invalid
        })),
        ("GET /api/unknown-resource", json!({})),
    ];
    
    for (endpoint, body) in invalid_cases {
        let response = make_request(endpoint, body).await;
        
        // Assert status is in 4xx or 5xx range
        assert!(response.status().is_client_error() 
            || response.status().is_server_error());
        
        // Assert Content-Type is application/problem+json
        let content_type = response.headers().get(CONTENT_TYPE).unwrap();
        assert_eq!(content_type, "application/problem+json; charset=utf-8");
        
        // Assert response body contains RFC 7807 fields
        let body: Value = serde_json::from_str(&response.text()).unwrap();
        assert!(body.get("type").is_some(), "Missing 'type' field");
        assert!(body.get("status").is_some(), "Missing 'status' field");
        assert!(body.get("detail").is_some(), "Missing 'detail' field");
    }
}
```

#### Phase 5 - Update API Docs (PR 5)
Update docs/api-usage.md with error response examples:

```markdown
## Error Responses

All errors follow [RFC 7807 Problem Details](https://tools.ietf.org/html/rfc7807) format:

### Example: Bad Request (400)
\`\`\`json
{
  "type": "https://api.sorobanpulse.dev/errors/bad-request",
  "status": 400,
  "detail": "Missing required field 'amount'",
  "instance": "/api/transfers"
}
\`\`\`

### Example: Not Found (404)
\`\`\`json
{
  "type": "https://api.sorobanpulse.dev/errors/not-found",
  "status": 404,
  "detail": "Event with ID 'abc123' does not exist",
  "instance": "/api/events/abc123"
}
\`\`\`

### Error Types

| Type URI | Status | Description |
|----------|--------|-------------|
| .../bad-request | 400 | Invalid request parameters |
| .../unauthorized | 401 | Missing or invalid authentication |
| .../forbidden | 403 | Authenticated but not authorized |
| .../not-found | 404 | Resource does not exist |
| .../conflict | 409 | Request conflicts with existing state |
| .../unprocessable | 422 | Validation error (see detail for field errors) |
| .../too-many-requests | 429 | Rate limit exceeded (see Retry-After header) |
| .../internal-error | 500 | Server error |
```

#### Phase 6 - CI Validation (PR 6)
Add to CI pipeline:

```yaml
# .github/workflows/error-validation.yml
- name: Validate error consistency
  run: |
    cargo test --test error_contract_tests
    # Verify no raw json!(...) error responses
    ! grep -r 'json!.*"error"' src/
    # Verify no ad-hoc (StatusCode, Json) returns
    ! grep -r 'Result.*StatusCode.*Json' src/
```

## Checklist

- [ ] Design final AppError variants
- [ ] Phase 1 PR: Add missing variants
- [ ] Phase 2 PR: Migrate all handlers
- [ ] Phase 3 PR: Remove ApiError type
- [ ] Phase 4 PR: Add contract tests
- [ ] Phase 5 PR: Update API docs
- [ ] Phase 6 PR: CI validation
- [ ] All handlers compile
- [ ] All tests pass
- [ ] curl/Postman tests pass

## Error Response Examples

### Complete Request/Response Cycle

```bash
# Client: Attempt invalid request
curl -X POST http://localhost:8080/api/transfers \
  -H "Content-Type: application/json" \
  -d '{"amount": -100}'

# Server Response (HTTP 400 Bad Request)
{
  "type": "https://api.sorobanpulse.dev/errors/validation-error",
  "status": 400,
  "detail": "Transfer amount must be positive",
  "instance": "/api/transfers",
  "errors": [
    {
      "field": "amount",
      "message": "Must be >= 0"
    }
  ]
}
```

```bash
# Client: Request with rate limiting
curl -X GET http://localhost:8080/api/events \
  -H "Authorization: Bearer token123"

# Server Response (HTTP 429 Too Many Requests)
HTTP/1.1 429 Too Many Requests
Retry-After: 60

{
  "type": "https://api.sorobanpulse.dev/errors/rate-limit-exceeded",
  "status": 429,
  "detail": "Rate limit: 100 requests per minute exceeded",
  "instance": "/api/events",
  "retry_after": 60
}
```

## Benefits

✅ Consistent error responses across all endpoints  
✅ Clients can reliably parse error format  
✅ Proper HTTP status codes for all scenarios  
✅ RFC 7807 compliance enables automation  
✅ OpenAPI/Swagger docs are accurate  
✅ Better debugging (consistent detail messages)  
✅ Type-safe error handling  
✅ Easier to add new error variants  

## Related Issues

Coordinates with:
- #1459 (feature gates) - Core error types remain in core feature
- #1460 (workspace) - Error types available to all workspace members
- #1461 (dependencies) - Error handling code stays simple/fast
