# API Compatibility Testing (Issue #592)

## Overview

API Compatibility Testing ensures that new API versions maintain backwards compatibility with previous versions. This prevents breaking changes that could affect existing clients and integrations.

## Strategy

### Version Matrix Testing

The project maintains a compatibility matrix across multiple API versions:

- **v1.0.0** - Initial API with core event retrieval
- **v1.1.0** - Adds ledger_hash field for transaction verification
- **v1.2.0** - Adds sequence field for event ordering

### Backwards Compatibility Rules

1. **Additive Only Changes**: New fields may only be added, never removed
2. **Field Type Stability**: Existing field types must not change
3. **Pagination Compatibility**: Pagination structure must remain compatible
4. **Error Response Consistency**: Error format must remain unchanged

### Testing Approach

The test suite validates:

- Response structure compatibility across versions
- Field presence in all responses
- Field type consistency
- Pagination format stability
- Filter schema evolution
- Error response format

## Running Compatibility Tests

```bash
# Run all compatibility tests
cargo test compatibility

# Run specific version test
cargo test compatibility_v1_1_adds_ledger_hash

# Run with output
cargo test compatibility -- --nocapture
```

## Compatibility Report

Generate a compatibility report by running:

```bash
cargo test compatibility_matrix_validates_backwards_compatibility -- --nocapture
```

## Deprecation Process

When deprecating an API endpoint:

1. **Announce Deprecation** - Publish deprecation notice with 6-month notice period
2. **Document Alternative** - Link to replacement endpoint in documentation
3. **Monitor Usage** - Track usage of deprecated endpoints
4. **Plan Removal** - Schedule removal after notice period expires

## API Versioning Strategy

### Semantic Versioning

- **Major (x.0.0)** - Breaking changes requiring client updates
- **Minor (1.x.0)** - Non-breaking additions (new fields, endpoints)
- **Patch (1.0.x)** - Bug fixes with no API changes

### Version Lifecycle

1. **GA (General Availability)** - Full support and updates
2. **Maintenance** - 12 months: Security fixes only
3. **Deprecated** - 6 months: No new features, deprecation warnings
4. **EOL (End of Life)** - Removed entirely

### Current API Support

| Version | Status | EOL Date |
|---------|--------|----------|
| v1.2.0  | GA     | 2027-06 |
| v1.1.0  | Maintenance | 2026-06 |
| v1.0.0  | Deprecated | 2026-01 |

## Examples

### Backwards Compatible Response

```json
{
  "success": true,
  "data": [
    {
      "id": "evt_123",
      "contract_id": "CAAAA...",
      "event_type": "contract",
      "ledger": 1000,
      "timestamp": "2024-01-01T12:00:00Z",
      "ledger_hash": "abc123",
      "sequence": 42
    }
  ],
  "pagination": {
    "page": 1,
    "limit": 50,
    "total": 1000,
    "has_next": true,
    "cursor": "cursor_token"
  }
}
```

All versions can parse this response because:
- v1.0.0 clients ignore unknown fields (ledger_hash, sequence, cursor)
- All required v1.0.0 fields are present
- Field types are consistent

## Maintenance

### Adding New Fields

When adding a new field:

1. Add to newest version first
2. Test with older clients (field ignored)
3. Document in migration guide
4. No database migration needed

### Removing Deprecated Fields

When removing a field:

1. Mark as deprecated (documentation only)
2. Wait 6 months minimum
3. Announce with version bump (major)
4. Remove in next major version

## Related Issues

- #556 - Contract Testing Framework
- #555 - API Documentation Generation
- #554 - OpenAPI Schema Validation
