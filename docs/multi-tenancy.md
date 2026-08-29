# Multi-Tenant Access Isolation - Issue #887

## Overview

SorobanPulse supports comprehensive multi-tenant support with row-level security (RLS) policies and strong tenant isolation guarantees. Each tenant has isolated access to their event data through both application-level and database-level controls.

## Architecture

### Multi-Tenant Model

- **Default Tenant**: All existing data assigned to "default" tenant for backward compatibility
- **Tenant Context**: Extracted from request headers (`x-tenant-id`)
- **RLS Policies**: PostgreSQL Row-Level Security enforces data isolation at the database level
- **Tenant Provisioning**: Dynamic tenant creation and configuration via API

## Components

### Models

```rust
pub struct Event {
    // ... existing fields
    pub tenant_id: String,  // Tenant ownership
}

pub struct PaginationParams {
    pub tenant_id: Option<String>,  // Filter by tenant
    // ... existing fields
}
```

### Tenant Context

```rust
pub struct TenantContext {
    pub tenant_id: TenantId,
    pub user_id: Option<String>,
    pub request_id: String,
}
```

### Tenant Provider

```rust
pub trait TenantProvider: Send + Sync {
    fn get_tenant(&self, tenant_id: &str) -> Option<TenantProvisioning>;
    fn provision_tenant(&self, provisioning: TenantProvisioning) -> Result<(), String>;
    fn list_tenants(&self) -> Vec<TenantProvisioning>;
}
```

## HTTP Headers

All requests should include tenant context:

```http
GET /events?limit=10
X-Tenant-ID: acme-corp
X-User-ID: user@acme-corp.com
```

## Middleware Integration

The tenant middleware automatically extracts tenant context:

```rust
use axum::{
    middleware,
    Router,
};
use soroban_pulse::middleware::tenant_context_middleware;

let app = Router::new()
    .layer(middleware::from_fn(tenant_context_middleware));
```

## Database Schema

### Tenant Tables

```sql
-- Tenant metadata
CREATE TABLE tenants (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    admin_email TEXT,
    config JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    enabled BOOLEAN DEFAULT TRUE
);

-- RLS policy definitions
CREATE TABLE tenant_rls_policies (
    id UUID PRIMARY KEY,
    tenant_id TEXT NOT NULL UNIQUE,
    policy_json JSONB NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

### Row-Level Security Policies

```sql
-- Example RLS policy on events table
CREATE POLICY tenant_isolation ON events
    FOR SELECT
    USING (tenant_id = current_setting('app.tenant_id'));

ALTER TABLE events ENABLE ROW LEVEL SECURITY;
```

## Tenant Provisioning

### Creating a New Tenant

```rust
use soroban_pulse::multi_tenancy::TenantProvisioning;

let provisioning = TenantProvisioning::new(
    "acme-corp".to_string(),
    "ACME Corporation".to_string(),
)
.with_admin_email("admin@acme-corp.com".to_string());

provider.provision_tenant(provisioning)?;
```

### Tenant Configuration

```rust
use serde_json::json;

let provisioning = TenantProvisioning::new(
    "tenant-id".to_string(),
    "Tenant Name".to_string(),
)
.with_config(json!({
    "encryption_enabled": true,
    "audit_logging": true,
    "retention_days": 90
}));
```

## Access Control

### Default Tenant Access

Requests without `X-Tenant-ID` header default to "default" tenant:

```rust
let ctx = TenantContext::from_header(None);
assert_eq!(ctx.tenant_id.as_str(), "default");
```

### Query Filtering

All queries automatically filter by tenant_id:

```rust
// Returns only events from "acme-corp" tenant
GET /events?limit=10 HTTP/1.1
X-Tenant-ID: acme-corp
```

## Security Auditing

### Audit Logging

All cross-tenant access attempts are logged:

```sql
SELECT * FROM tenant_access_audit
WHERE tenant_id = 'acme-corp'
ORDER BY created_at DESC;
```

### RLS Violation Detection

The database prevents RLS violations at the storage layer:

```sql
-- This query will return no results if tenant_id doesn't match session
SELECT * FROM events
WHERE tenant_id = 'competitor-corp';
```

## API Endpoints

### List Tenants

```http
GET /api/admin/tenants
Authorization: Bearer <admin-token>
```

### Provision Tenant

```http
POST /api/admin/tenants
Authorization: Bearer <admin-token>
Content-Type: application/json

{
  "tenant_id": "new-tenant",
  "name": "New Tenant Inc",
  "admin_email": "admin@new-tenant.com"
}
```

### Inspect Tenant Config

```http
GET /api/admin/tenants/{tenant_id}
Authorization: Bearer <admin-token>
X-Tenant-ID: {tenant_id}
```

## Testing

### Unit Tests

```bash
cargo test multi_tenancy::tests
```

Tests cover:
- Tenant ID creation and validation
- Tenant context extraction from headers
- Tenant provisioning lifecycle
- Default tenant behavior

### Integration Tests

```bash
cargo test --test '*multi_tenant*'
```

Scenarios:
- Cross-tenant isolation (verify isolation failures)
- Concurrent tenant access
- RLS policy enforcement
- Audit logging of access attempts

## Migration Guide

### Migrating Existing Data

1. All existing events assigned to "default" tenant
2. Run migration to add `tenant_id` column with default value
3. Existing API clients continue to work without changes
4. Enable RLS policies for new deployments

```sql
-- Migration adds tenant_id with default
ALTER TABLE events
ADD COLUMN tenant_id TEXT NOT NULL DEFAULT 'default';

-- Create indexes for performance
CREATE INDEX idx_events_tenant_id ON events(tenant_id);
CREATE INDEX idx_events_tenant_contract ON events(tenant_id, contract_id);
```

## Compliance & Security

- **SOC 2 Type II**: Multi-tenant isolation controls
- **GDPR**: Tenant-level data subject rights
- **HIPAA**: Tenant isolation for healthcare data
- **PCI DSS**: Cardholder data isolation per tenant

## Monitoring

### Tenant Metrics

```rust
// Monitor per-tenant activity
metrics::record_tenant_request(tenant_id);
metrics::record_tenant_query_count(tenant_id, count);
```

### Alerts

Configure alerts for:
- RLS policy violations
- Unauthorized cross-tenant access attempts
- Tenant provisioning failures
- Unusual tenant activity patterns

## Troubleshooting

**Access denied errors**: Check X-Tenant-ID header value
**RLS violations**: Verify database session tenant_id setting
**Tenant creation fails**: Check admin permissions and database constraints
**Queries return no results**: Verify tenant_id matches requested tenant

## References

- [PostgreSQL Row-Level Security](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [SaaS Multi-Tenancy Patterns](https://docs.microsoft.com/en-us/azure/sql-database/sql-database-multi-tenant-application)
