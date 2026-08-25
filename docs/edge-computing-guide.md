# Edge Computing & CDN Integration Guide

## Overview

This guide covers deploying SorobanPulse with edge computing and CDN solutions to reduce latency, improve performance, and enhance global availability.

## Architecture

```
┌─────────────┐
│   Users     │
└──────┬──────┘
       │
┌──────▼────────────────────────┐
│  Edge Layer (CDN/Workers)     │
│  - Caching                    │
│  - Request routing            │
│  - Edge processing            │
└──────┬────────────────────────┘
       │
┌──────▼────────────────────────┐
│  Global Load Balancer         │
│  - AWS Global Accelerator     │
│  - Multi-region routing       │
└──────┬────────────────────────┘
       │
┌──────▼────────────────────────┐
│  Regional Deployments         │
│  - US East (Primary)          │
│  - EU West (Secondary)        │
│  - AP Southeast (Tertiary)    │
└───────────────────────────────┘
```

## Cloudflare Integration

### Benefits

- **Global Edge Network**: 300+ locations worldwide
- **DDoS Protection**: Built-in security
- **Smart Caching**: Intelligent cache management
- **Workers**: Serverless edge computing
- **Analytics**: Real-time performance insights

### Setup

#### 1. Install Wrangler CLI

```bash
npm install -g wrangler

# Login to Cloudflare
wrangler login
```

#### 2. Configure Worker

```bash
# Navigate to edge directory
cd edge/

# Update wrangler.toml with your account details
# Edit: account_id, zone_id, and kv_namespace_id
```

#### 3. Deploy Worker

```bash
# Develop locally
wrangler dev

# Deploy to staging
wrangler publish --env staging

# Deploy to production
wrangler publish --env production
```

### Worker Features

#### Intelligent Caching

```javascript
// Automatic caching with TTL
const CACHE_TTL = {
  ledgers: 300,      // 5 minutes
  transactions: 600,  // 10 minutes
  events: 60,        // 1 minute
};

// Cache warming
async function warmCache() {
  const popularEndpoints = [
    '/api/v1/ledgers/latest',
    '/api/v1/events?limit=100',
  ];
  
  for (const endpoint of popularEndpoints) {
    await fetch(`${ORIGIN}${endpoint}`);
  }
}
```

#### Request Routing

```javascript
// Route based on geography
const REGIONAL_ORIGINS = {
  'US': 'https://us-api.soroban-pulse.example.com',
  'EU': 'https://eu-api.soroban-pulse.example.com',
  'AS': 'https://as-api.soroban-pulse.example.com',
};

function getRegionalOrigin(country) {
  const continent = getContinent(country);
  return REGIONAL_ORIGINS[continent] || REGIONAL_ORIGINS['US'];
}
```

#### Rate Limiting

```javascript
// Edge-side rate limiting
const RATE_LIMITS = {
  '/api/v1/ledgers': 100,    // 100 req/min
  '/api/v1/transactions': 50, // 50 req/min
  '/graphql': 20,             // 20 req/min
};

async function checkRateLimit(ip, path) {
  const key = `ratelimit:${ip}:${path}`;
  const count = await KV.get(key) || 0;
  
  if (count > RATE_LIMITS[path]) {
    return { allowed: false };
  }
  
  await KV.put(key, count + 1, { expirationTtl: 60 });
  return { allowed: true };
}
```

### Cache Purging

```bash
# Purge all cache
curl -X POST "https://api.cloudflare.com/client/v4/zones/${ZONE_ID}/purge_cache" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: application/json" \
  --data '{"purge_everything":true}'

# Purge specific URLs
curl -X POST "https://api.cloudflare.com/client/v4/zones/${ZONE_ID}/purge_cache" \
  -H "Authorization: Bearer ${CF_API_TOKEN}" \
  -H "Content-Type: application/json" \
  --data '{"files":["https://api.soroban-pulse.example.com/api/v1/ledgers/12345"]}'
```

## AWS CloudFront Integration

### Benefits

- **AWS Integration**: Native AWS services integration
- **Custom SSL**: Bring your own certificates
- **Lambda@Edge**: Serverless edge computing
- **Cost Effective**: Pay-as-you-go pricing
- **Global Coverage**: 400+ edge locations

### Deployment

#### 1. Deploy with Terraform

```bash
cd terraform/

# Initialize Terraform
terraform init

# Plan deployment
terraform plan -var-file=production.tfvars

# Apply configuration
terraform apply -var-file=production.tfvars
```

#### 2. Configure DNS

```bash
# Point domain to CloudFront distribution
aws route53 change-resource-record-sets \
  --hosted-zone-id ${ZONE_ID} \
  --change-batch file://dns-change.json
```

### Cache Behaviors

#### Static Content (High TTL)

- OpenAPI specs
- Documentation
- SDK downloads
- **TTL**: 24 hours

#### Dynamic Content (Medium TTL)

- Ledger data (finalized)
- Transaction history
- Contract information
- **TTL**: 5-10 minutes

#### Real-time Content (No Cache)

- Latest ledger
- Event streams
- WebSocket connections
- **TTL**: 0 (no cache)

### Lambda@Edge Functions

#### Request Transformation

```javascript
// CloudFront Function - Viewer Request
function handler(event) {
    var request = event.request;
    
    // Add security headers
    request.headers['x-content-type-options'] = { value: 'nosniff' };
    request.headers['x-frame-options'] = { value: 'DENY' };
    request.headers['x-xss-protection'] = { value: '1; mode=block' };
    
    // Route API versions
    if (request.uri.startsWith('/api/v2/')) {
        request.origin.custom.domainName = 'v2-api.soroban-pulse.example.com';
    }
    
    return request;
}
```

#### Response Modification

```javascript
// Lambda@Edge - Origin Response
exports.handler = async (event) => {
    const response = event.Records[0].cf.response;
    
    // Add CORS headers
    response.headers['access-control-allow-origin'] = [{
        key: 'Access-Control-Allow-Origin',
        value: '*'
    }];
    
    // Add cache headers based on content
    if (response.status === '200') {
        const uri = event.Records[0].cf.request.uri;
        if (uri.includes('/ledgers/')) {
            response.headers['cache-control'] = [{
                key: 'Cache-Control',
                value: 'public, max-age=300'
            }];
        }
    }
    
    return response;
};
```

### Monitoring

```bash
# View CloudFront metrics
aws cloudwatch get-metric-statistics \
  --namespace AWS/CloudFront \
  --metric-name Requests \
  --dimensions Name=DistributionId,Value=${DISTRIBUTION_ID} \
  --start-time 2024-01-01T00:00:00Z \
  --end-time 2024-01-02T00:00:00Z \
  --period 3600 \
  --statistics Sum

# View cache hit rate
aws cloudfront get-distribution-config \
  --id ${DISTRIBUTION_ID} \
  --query 'DistributionConfig.CacheBehaviors'
```

## Multi-Region Deployment

### AWS Global Accelerator

#### Benefits

- **Static IPs**: Two global static IP addresses
- **Health Checks**: Automatic failover
- **Performance**: Optimal path selection
- **DDoS Protection**: AWS Shield Standard included

#### Configuration

```bash
# Deploy multi-region infrastructure
cd terraform/
terraform apply -target=module.soroban_pulse_us_east
terraform apply -target=module.soroban_pulse_eu_west
terraform apply -target=module.soroban_pulse_ap_southeast

# Deploy Global Accelerator
terraform apply -target=aws_globalaccelerator_accelerator.soroban_pulse
```

#### Traffic Distribution

- **Primary (US East)**: 50% weight
- **Secondary (EU West)**: 30% weight
- **Tertiary (AP Southeast)**: 20% weight

### Health Checks

```yaml
# Health check configuration
health_check:
  protocol: HTTPS
  port: 443
  path: /health
  interval: 30s
  timeout: 10s
  healthy_threshold: 3
  unhealthy_threshold: 3
```

### Failover Strategy

1. **Automatic Failover**: Based on health checks
2. **Manual Override**: Via Terraform or AWS Console
3. **Gradual Recovery**: Progressive traffic shift back
4. **Monitoring**: CloudWatch alarms for failures

## Performance Optimization

### Cache Strategy

#### Cache-Control Headers

```rust
// In application code
use actix_web::http::header;

// Immutable resources
response.headers_mut().insert(
    header::CACHE_CONTROL,
    header::HeaderValue::from_static("public, max-age=31536000, immutable")
);

// Frequently updated
response.headers_mut().insert(
    header::CACHE_CONTROL,
    header::HeaderValue::from_static("public, max-age=300, must-revalidate")
);

// Real-time
response.headers_mut().insert(
    header::CACHE_CONTROL,
    header::HeaderValue::from_static("no-cache, no-store, must-revalidate")
);
```

#### Conditional Requests

```rust
// Support ETag and If-None-Match
let etag = format!("\"{}\"", hash_content(&body));
response.headers_mut().insert(
    header::ETAG,
    header::HeaderValue::from_str(&etag)?
);

// Check If-None-Match
if let Some(if_none_match) = req.headers().get(header::IF_NONE_MATCH) {
    if if_none_match.to_str()? == etag {
        return Ok(HttpResponse::NotModified().finish());
    }
}
```

### Compression

```javascript
// CloudFront automatic compression
{
  "compress": true,
  "compressTypes": [
    "text/html",
    "application/json",
    "application/javascript",
    "text/css"
  ]
}
```

### Origin Shield

```terraform
# Enable Origin Shield to reduce load on origin
origin_shield {
  enabled              = true
  origin_shield_region = "us-east-1"
}
```

## Security

### DDoS Protection

- **Cloudflare**: Automatic DDoS mitigation
- **AWS Shield Standard**: Included with CloudFront
- **AWS Shield Advanced**: Optional enhanced protection

### Web Application Firewall (WAF)

```bash
# Create WAF rules
aws wafv2 create-web-acl \
  --name soroban-pulse-waf \
  --scope CLOUDFRONT \
  --default-action Allow={} \
  --rules file://waf-rules.json

# Associate with CloudFront
aws wafv2 associate-web-acl \
  --web-acl-arn ${WAF_ARN} \
  --resource-arn ${CF_ARN}
```

### Bot Management

```javascript
// Cloudflare Bot Management
if (request.headers.get('cf-bot-management-score') < 30) {
    return new Response('Bot detected', { status: 403 });
}
```

## Monitoring & Analytics

### CloudFlare Analytics

```bash
# Access via Cloudflare dashboard
# - Requests per second
# - Bandwidth usage
# - Cache hit ratio
# - Top endpoints
# - Geographic distribution
```

### CloudFront Logs

```bash
# Enable logging
aws cloudfront update-distribution \
  --id ${DISTRIBUTION_ID} \
  --distribution-config file://logging-config.json

# Query logs with Athena
aws athena start-query-execution \
  --query-string "SELECT * FROM cloudfront_logs WHERE status = 200 LIMIT 100" \
  --result-configuration OutputLocation=s3://my-results-bucket/
```

### Custom Metrics

```javascript
// Export metrics from worker
export default {
  async fetch(request, env) {
    const start = Date.now();
    const response = await handleRequest(request);
    const duration = Date.now() - start;
    
    // Send to metrics endpoint
    await env.METRICS.put(`latency:${request.url}`, duration);
    
    return response;
  }
}
```

## Cost Optimization

### CloudFront Cost Reduction

1. **Origin Shield**: Reduce origin requests
2. **Compression**: Reduce bandwidth costs
3. **Price Class**: Select regions strategically
4. **Reserved Capacity**: For predictable workloads

### Cloudflare Cost Management

1. **Worker KV**: $0.50 per million reads
2. **Worker CPU**: $0.30 per million requests
3. **Bandwidth**: Varies by plan (Pro/Business/Enterprise)

## Best Practices

1. **Cache Everything Possible**: Maximize cache hit rates
2. **Use ETags**: Enable conditional requests
3. **Compress Content**: Reduce bandwidth costs
4. **Monitor Metrics**: Track performance continuously
5. **Test Failover**: Regularly test multi-region failover
6. **Secure Origins**: Restrict direct origin access
7. **Version APIs**: Support multiple API versions at edge

## Troubleshooting

### High Cache Miss Rate

```bash
# Check cache key configuration
# Verify vary headers
# Review query string forwarding
# Analyze cache-control headers
```

### Increased Latency

```bash
# Check origin health
# Review edge location performance
# Verify DNS resolution
# Check for rate limiting
```

### Purge Not Working

```bash
# Verify purge API credentials
# Check cache key format
# Wait for propagation (up to 5 minutes)
# Use wildcard purging for patterns
```

## Resources

- [Cloudflare Workers Documentation](https://developers.cloudflare.com/workers/)
- [AWS CloudFront Documentation](https://docs.aws.amazon.com/cloudfront/)
- [AWS Global Accelerator Documentation](https://docs.aws.amazon.com/global-accelerator/)
- [CDN Performance Best Practices](https://www.cdnplanet.com/blog/)

## Support

For edge computing and CDN support:
- GitHub Issues: https://github.com/Soroban-Pulse/SorobanPulse/issues
- Community Slack: #edge-computing
- Email: support@soroban-pulse.example.com
