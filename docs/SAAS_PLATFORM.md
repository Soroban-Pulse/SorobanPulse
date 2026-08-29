# SaaS Platform & Multi-Tenant Hosting

**Issue #841**

This document describes the SaaS platform features for managed multi-tenant hosting of SorobanPulse.

## Overview

The SaaS platform enables hosting SorobanPulse as a managed service with:

- **Tenant Provisioning**: Automated onboarding and account creation
- **Subscription Management**: Multiple tiers with different resource limits
- **Billing Integration**: Hooks for Stripe, Paddle, or custom billing systems
- **Usage Tracking**: Monitor API requests, events, and resource consumption
- **Quota Enforcement**: Automatic enforcement of plan limits
- **Tenant Isolation**: Complete data and resource isolation between tenants

## Architecture

### Multi-Tenancy Model

SorobanPulse uses a **shared database with tenant isolation** model:

- All tenants share the same database and application instances
- Data is isolated using `tenant_id` fields in all tables
- Row-level security ensures queries are automatically scoped to the correct tenant
- API keys are mapped to specific tenants for authentication

### Subscription Tiers

Five subscription tiers are available:

| Tier | API Requests/Day | Events/Month | Subscriptions | Webhooks | Storage | SLA | Support |
|------|-----------------|--------------|---------------|----------|---------|-----|---------|
| **Free** | 1,000 | 10,000 | 5 | 2 | 1 GB | 95% | Community |
| **Starter** | 10,000 | 100,000 | 20 | 10 | 10 GB | 99% | Email |
| **Professional** | 100,000 | 1,000,000 | 100 | 50 | 50 GB | 99.5% | Priority |
| **Enterprise** | 1,000,000 | 10,000,000 | 500 | 200 | 500 GB | 99.9% | Dedicated |
| **Custom** | Unlimited | Unlimited | Unlimited | Unlimited | Custom | 99.99% | 24/7 |

## Configuration

### Environment Variables

```bash
# Enable SaaS platform features
SAAS_ENABLED=true

# Billing provider (stripe, paddle, manual)
BILLING_PROVIDER=stripe
BILLING_API_KEY=sk_live_xxxxx

# Default trial period for new tenants (days)
DEFAULT_TRIAL_DAYS=14

# Public base URL for tenant onboarding
SAAS_PUBLIC_BASE_URL=https://pulse.example.com

# Multi-tenant mode must be enabled
MULTI_TENANT=true
```

### Database Setup

Run the migration to create SaaS tables:

```bash
psql $DATABASE_URL < migrations/20250826_saas_ml_features.sql
```

## API Endpoints

### Provisioning

#### POST `/v1/admin/tenants`

Provision a new tenant account.

**Request:**
```json
{
  "organization_name": "Acme Corporation",
  "contact_email": "admin@acme.com",
  "subscription_tier": "professional",
  "custom_domain": "pulse.acme.com",
  "trial_days": 14,
  "metadata": {
    "signup_source": "website",
    "referral_code": "PARTNER123"
  }
}
```

**Response:**
```json
{
  "tenant_id": "tenant_a1b2c3d4e5f6",
  "api_key": "sk_live_xxxxxxxxxxxxx",
  "admin_api_key": "sk_admin_xxxxxxxxxxxxx",
  "subscription_tier": "professional",
  "trial_ends_at": "2026-09-09T00:00:00Z",
  "onboarding_url": "/onboard/tenant_a1b2c3d4e5f6"
}
```

### Management

#### GET `/v1/admin/tenants`

List all tenants with optional filtering.

**Query Parameters:**
- `status`: Filter by status (active, suspended, trial, etc.)
- `tier`: Filter by subscription tier
- `limit`: Page size (default: 50)
- `offset`: Pagination offset

#### GET `/v1/admin/tenants/:tenant_id`

Get details for a specific tenant.

#### PUT `/v1/admin/tenants/:tenant_id/tier`

Update a tenant's subscription tier.

**Request:**
```json
{
  "new_tier": "enterprise"
}
```

#### POST `/v1/admin/tenants/:tenant_id/suspend`

Suspend a tenant (e.g., for non-payment).

**Request:**
```json
{
  "reason": "Payment failed"
}
```

#### POST `/v1/admin/tenants/:tenant_id/reactivate`

Reactivate a suspended tenant.

### Usage Tracking

#### GET `/v1/admin/tenants/:tenant_id/usage`

Get usage statistics for a tenant.

**Query Parameters:**
- `period_start`: Start of period (ISO 8601)
- `period_end`: End of period (ISO 8601)

**Response:**
```json
{
  "tenant_id": "tenant_a1b2c3d4e5f6",
  "period_start": "2026-08-01T00:00:00Z",
  "period_end": "2026-08-31T23:59:59Z",
  "api_requests": 45230,
  "events_indexed": 123456,
  "webhooks_sent": 8942,
  "storage_used_gb": 12.5,
  "active_subscriptions": 15,
  "bandwidth_gb": 45.2
}
```

#### GET `/v1/admin/tenants/:tenant_id/quota-status`

Check if tenant has exceeded any quotas.

**Response:**
```json
{
  "tenant_id": "tenant_a1b2c3d4e5f6",
  "quotas": {
    "api_requests": {
      "limit": 100000,
      "used": 45230,
      "percentage": 45.23,
      "exceeded": false
    },
    "events": {
      "limit": 1000000,
      "used": 123456,
      "percentage": 12.35,
      "exceeded": false
    }
  }
}
```

## Billing Integration

### Webhook Events

The SaaS platform can receive webhook events from billing providers:

#### POST `/v1/webhooks/billing`

Handle billing system webhooks (Stripe, Paddle, etc.).

**Supported Events:**
- `subscription.created`: New subscription created
- `subscription.upgraded`: Tier upgraded
- `subscription.downgraded`: Tier downgraded
- `subscription.cancelled`: Subscription cancelled
- `payment.succeeded`: Payment successful
- `payment.failed`: Payment failed
- `trial.ending`: Trial period ending soon (3 days before)
- `trial.ended`: Trial period ended

### Example Integration

```rust
use soroban_pulse::saas_platform::billing::*;

// Handle Stripe webhook
let webhook = BillingWebhook {
    event_type: "payment.succeeded".to_string(),
    tenant_id: "tenant_a1b2c3d4e5f6".to_string(),
    billing_id: "sub_1234567890".to_string(),
    amount: Some(99.00),
    currency: Some("USD".to_string()),
    timestamp: Utc::now(),
    metadata: serde_json::json!({
        "invoice_id": "in_1234567890"
    }),
};

handle_billing_webhook(&pool, webhook).await?;
```

## Tenant Isolation

### Database-Level Isolation

All queries are automatically scoped to the authenticated tenant:

```sql
-- Automatically adds WHERE tenant_id = 'tenant_xxx'
SELECT * FROM events;
```

### API Key Mapping

API keys are mapped to tenants in the `api_key_tenants` table:

```rust
// Middleware extracts tenant_id from API key
let tenant_id = middleware::extract_tenant_id(&api_key)?;

// All subsequent queries use this tenant_id
let events = db::get_events(&pool, &tenant_id, filters).await?;
```

### Rate Limiting

Per-tenant rate limits are enforced:

```rust
if config.multi_tenant {
    // Each tenant gets their own rate limit bucket
    rate_limiter.check_tenant_limit(&tenant_id)?;
} else {
    // Global rate limit
    rate_limiter.check_global_limit()?;
}
```

## Self-Service Onboarding

### Onboarding Flow

1. User signs up at `/signup`
2. System provisions new tenant
3. User receives welcome email with:
   - Tenant ID
   - API keys
   - Onboarding link
4. User completes onboarding wizard:
   - Set up first subscription
   - Configure webhooks
   - Test integration
5. Trial period begins

### Onboarding UI

```typescript
// Example onboarding component
const OnboardingWizard = ({ tenantId, apiKey }) => {
  const steps = [
    { title: "Welcome", component: <Welcome /> },
    { title: "Create Subscription", component: <CreateSubscription /> },
    { title: "Configure Webhooks", component: <ConfigureWebhooks /> },
    { title: "Test Integration", component: <TestIntegration /> },
    { title: "Complete", component: <Complete /> },
  ];
  
  return <StepWizard steps={steps} />;
};
```

## Monitoring & Analytics

### Dashboard Metrics

Track key SaaS metrics:

- **MRR (Monthly Recurring Revenue)**: By tier
- **Churn Rate**: Percentage of cancelled subscriptions
- **Customer Lifetime Value (CLV)**: Average revenue per tenant
- **Tenant Growth**: New signups over time
- **Usage by Tier**: Resource consumption patterns
- **Quota Violations**: Tenants exceeding limits

### Alerts

Configure alerts for:

- Tenant approaching quota limits
- Payment failures
- Trial expiring soon
- Unusual usage patterns
- System-wide capacity thresholds

## Best Practices

### Security

1. **API Key Rotation**: Encourage tenants to rotate keys regularly
2. **Audit Logging**: Log all admin operations on tenants
3. **Access Control**: Use admin API keys for management operations
4. **HTTPS Only**: Enforce HTTPS for all API endpoints
5. **Secret Management**: Never log or expose API keys

### Performance

1. **Connection Pooling**: Use per-tenant connection pools
2. **Query Optimization**: Add indexes on `tenant_id` columns
3. **Caching**: Cache tenant metadata and configuration
4. **Async Processing**: Use background jobs for usage tracking
5. **Horizontal Scaling**: Add replicas for read-heavy workloads

### Operations

1. **Backup Strategy**: Regular backups with tenant isolation
2. **Migration Tools**: Provide tenant export/import functionality
3. **Support Portal**: Self-service portal for common tasks
4. **Documentation**: Comprehensive API and integration guides
5. **Status Page**: Public status page for service health

## Troubleshooting

### Common Issues

**Problem**: Tenant can't authenticate

**Solution**: Check API key mapping in `api_key_tenants` table:
```sql
SELECT * FROM api_key_tenants WHERE api_key_hash = hash_api_key('sk_live_xxx');
```

**Problem**: Quota exceeded errors

**Solution**: Check current usage and adjust limits:
```sql
SELECT * FROM saas_tenants WHERE tenant_id = 'tenant_xxx';
```

**Problem**: Billing webhook not processing

**Solution**: Check webhook signature verification and event type handling.

## Migration from Self-Hosted

To migrate existing self-hosted installations to SaaS:

1. Enable multi-tenant mode
2. Create tenant for existing installation
3. Update events and subscriptions with tenant_id
4. Generate API keys
5. Update client configurations

```sql
-- Migrate existing data to tenant
UPDATE events SET tenant_id = 'tenant_existing' WHERE tenant_id IS NULL;
UPDATE subscriptions SET tenant_id = 'tenant_existing' WHERE tenant_id IS NULL;
```

## Future Enhancements

- [ ] Custom branding per tenant
- [ ] White-label deployments
- [ ] Enterprise SSO integration
- [ ] Advanced usage analytics
- [ ] Cost allocation reports
- [ ] Automated capacity planning
- [ ] Tenant cloning/templating
- [ ] Geographic data residency options

## See Also

- [Multi-Tenant Architecture](MULTI_TENANT_ARCHITECTURE.md)
- [Billing Integration Guide](BILLING_INTEGRATION.md)
- [Security Best Practices](SECURITY.md)
- [API Documentation](API.md)
