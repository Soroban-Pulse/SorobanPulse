# Capacity Planning Automation (Issue #589)

## Overview

Capacity Planning Automation predicts resource requirements and alerts on capacity thresholds. It analyzes growth trends, creates forecasts, and generates resource recommendations to prevent outages and optimize infrastructure spending.

## Features

### Growth Trend Analysis

Analyzes resource usage patterns to identify:

- **Increasing Trends** - Usage growing > 5% per period
- **Stable Trends** - Usage stable ± 5% per period
- **Decreasing Trends** - Usage declining > 5% per period

### Forecasting

Projects future resource needs:

- **7-day Forecast** - Near-term trend prediction
- **30-day Forecast** - Monthly capacity planning
- **Growth Rate** - Percentage change per period

### Resource Recommendations

Auto-generates recommendations with:

- **Current Usage** - Actual resource consumption
- **Recommended Capacity** - Needed capacity (with 20% buffer)
- **Urgency Level** - Critical, High, Medium, Low
- **Forecast Data** - Reasoning with projected usage

### Capacity Alerts

Real-time alerts when:

- **Alert Threshold** - 80% of capacity
- **Critical Level** - 100% of capacity
- **Severity Levels** - Critical, Warning, Info

## Tracked Metrics

| Metric          | Capacity | Alert Threshold | Period    |
| --------------- | -------- | --------------- | --------- |
| DB Connections  | 100      | 80              | Real-time |
| Memory (GB)     | 8        | 6.4             | Real-time |
| Storage (GB)    | 1024     | 819.2           | Hourly    |
| Events/sec      | 10000    | 8000            | Real-time |
| API Connections | 1000     | 750             | Real-time |

## Using Capacity Planner

### Basic Usage

```rust
use soroban_pulse::capacity_planning::CapacityPlanner;

let mut planner = CapacityPlanner::new();

// Record metric
planner.record_metric("db_connections".to_string(), 50.0);
planner.record_metric("db_connections".to_string(), 75.0);

// Analyze trend
if let Some(trend) = planner.analyze_growth_trend("db_connections") {
    println!("Growth: {:.1}%", trend.growth_rate_percent);
    println!("7-day forecast: {:.0}", trend.forecast_7days);
    println!("30-day forecast: {:.0}", trend.forecast_30days);
}
```

### Generate Recommendations

```rust
// Get single resource recommendation
if let Some(rec) = planner.generate_recommendation("memory_gb") {
    println!("Current: {:.1}GB", rec.current_capacity);
    println!("Recommended: {:.1}GB", rec.recommended_capacity);
    println!("Urgency: {:?}", rec.urgency);
}

// Get all recommendations
let recommendations = planner.get_recommendations_summary();
for rec in recommendations {
    if rec.urgency == CapacityUrgency::Critical {
        send_alert(&rec);
    }
}
```

### Check Alerts

```rust
// Check single metric
if let Some(alert) = planner.check_alert_threshold("storage_gb") {
    println!("Alert: {}", alert.message);
}

// Get all active alerts
let alerts = planner.get_active_alerts();
for alert in alerts {
    if alert.severity == AlertSeverity::Critical {
        notify_oncall(&alert);
    }
}
```

### Generate Forecast

```rust
// Generate 30-day forecast
if let Some(forecast) = planner.generate_forecast("events_per_sec", 30) {
    for (day, predicted_value) in forecast.forecast_data {
        println!("Day {}: {:.0} events/sec", day, predicted_value);
    }
}
```

## Capacity Forecast Dashboard

### Dashboard Components

1. **Current Usage** - Real-time resource consumption
2. **Forecast Graph** - 30-day trend projection
3. **Recommendations** - Highlighted capacity upgrades
4. **Alerts** - Active and historical alerts
5. **Trend Summary** - Growth rates by resource

### Accessing Dashboard

```bash
# View dashboard in browser
open http://localhost:3000/dashboards/capacity-planning

# Or via API
curl http://localhost:3000/api/v1/capacity/forecast?days=30

# With specific metric
curl http://localhost:3000/api/v1/capacity/forecast?metric=memory_gb&days=30
```

## Alert Configuration

### Setting Custom Thresholds

Configure in `config.toml`:

```toml
[capacity_planning]
# Database connections
db_connections_capacity = 100
db_connections_alert_percent = 80  # Alert at 80 connections

# Memory in GB
memory_capacity_gb = 8.0
memory_alert_percent = 80  # Alert at 6.4GB

# Storage in GB
storage_capacity_gb = 1024.0
storage_alert_percent = 80  # Alert at 819.2GB

# Event throughput
events_per_sec_capacity = 10000
events_per_sec_alert_percent = 80

# API concurrency
api_connections_capacity = 1000
api_connections_alert_percent = 75  # Alert at 750 connections

# Forecast interval (days)
forecast_days = 30

# Urgency thresholds
critical_days_remaining = 7  # < 7 days = critical
high_days_remaining = 14     # 7-14 days = high
medium_days_remaining = 30   # 14-30 days = medium
```

## Automation Integration

### Scheduled Capacity Analysis

Runs every hour:

```bash
# In crontab or scheduler
0 * * * * ./scripts/capacity_analysis.sh

# Manual trigger
./scripts/capacity_analysis.sh --force
```

### Alert Routing

Alerts route to:

1. **Application Logs** - All alerts logged
2. **Prometheus** - Metrics scraped by monitoring
3. **Email** - Critical alerts to ops team
4. **Slack** - High urgency alerts to #ops channel
5. **PagerDuty** - Critical cascaded alerts

### Auto-Scaling Integration

Recommendations can trigger auto-scaling:

```bash
# Query recommendations API
curl http://localhost:3000/api/v1/capacity/recommendations

# Output:
# {
#   "resource": "db_connections",
#   "recommended_capacity": 150,
#   "urgency": "HIGH"
# }

# Trigger scaling event
./scripts/scale_resource.sh db_connections 150
```

## Forecasting Algorithm

### Growth Rate Calculation

```
growth_rate = ((current - previous) / previous) * 100%

Trend Direction:
- Increasing: > 5%
- Stable: ±5%
- Decreasing: < -5%
```

### Capacity Forecast

```
forecast_value = current_value + (daily_change * days_ahead)

Where:
daily_change = current_value - previous_value

Example:
current = 100
previous = 90
daily_change = 10
forecast_7_days = 100 + (10 * 7) = 170
```

### Urgency Calculation

```
days_to_capacity = (capacity - current) / daily_change

Urgency:
- Critical: < 7 days
- High: 7-14 days
- Medium: 14-30 days
- Low: > 30 days
```

## Common Scenarios

### Database Connection Pool

```
Current: 85 connections
Capacity: 100
Growth: +5 connections/day
Forecast 7d: 120 connections

Result: CRITICAL - Exceeds capacity in 3 days
Recommendation: Increase to 150 connections
```

### Storage Growth

```
Current: 800GB
Capacity: 1024GB (1TB)
Growth: +10GB/day
Forecast 30d: 1100GB

Result: HIGH - Exceeds capacity in 22 days
Recommendation: Increase to 2TB
```

### Event Processing

```
Current: 7500 events/sec
Capacity: 10000 events/sec
Growth: -500 events/sec (declining)
Forecast 30d: 5000 events/sec

Result: LOW - No action needed
Status: Trending better than expected
```

## Monitoring and Adjustments

### Review Metrics

Daily review in dashboard shows:

- Accuracy of previous forecasts
- Actual vs. predicted usage
- Trend changes

### Adjust Forecasts

If prediction accuracy < 80%:

1. Increase data points collected
2. Review for seasonal patterns
3. Account for planned events (releases, migrations)

### Update Thresholds

Quarterly threshold review:

- Validate alert thresholds still appropriate
- Adjust for known seasonal patterns
- Update capacity limits based on business growth

## Integration Examples

### Kubernetes Auto-Scaling

```yaml
apiVersion: autoscaling.knative.dev/v1alpha1
kind: KPA
metadata:
  name: soroban-pulse
spec:
  scaleTargetRef:
    name: soroban-pulse
  maxScaleDownRate: "0.1"
  # Use capacity planning metrics
  metrics:
    - type: Resource
      resource:
        name: cpu
        target:
          type: Utilization
          averageUtilization: "70"
```

### CloudWatch Integration (AWS)

```python
# Put custom metrics
cloudwatch.put_metric_data(
    Namespace='SorobanPulse',
    MetricData=[
        {
            'MetricName': 'DatabaseConnections',
            'Value': 85,
            'Unit': 'Count'
        },
        {
            'MetricName': 'StorageUsageGB',
            'Value': 800,
            'Unit': 'Gigabytes'
        }
    ]
)
```

## Related Issues

- #588 - Performance Monitoring Dashboard
- #587 - Auto-scaling Policies
- #591 - Performance Regression Testing
