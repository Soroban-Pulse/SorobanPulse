# SLI / SLO Dashboard (Issue #696)

This document defines SorobanPulse's **Service Level Indicators (SLIs)** and
**Service Level Objectives (SLOs)** and explains how the in-process tracker,
Prometheus metrics, Grafana dashboard, and Prometheus alerts fit together.

## Concepts

* **SLI — Service Level Indicator.** A quantitative measurement of the level
  of service provided. Examples: `p99 request latency`, `error rate`, `uptime`.
* **SLO — Service Level Objective.** A target value (or range) for an SLI over
  a defined window. Example: *"99% of HTTP requests will return within 250 ms
  measured at the server over any rolling 30-day window."*
* **Error budget.** `1 − SLO target`. If the SLO target is 99%, the error
  budget is 1% (the share of requests we may "fail" without violating the SLO).
* **Burn rate.** The normalized rate of budget consumption. A burn rate of `1.0`
  means the budget will be exhausted exactly at the end of the window if the
  current bad-event rate persists. Burn rates > 2 trigger fast-burn alerts
  (Google SRE workbook guidance).

## Implementation layer

| Layer | Component | Purpose |
|---|---|---|
| In-process | `src/slo_tracker.rs` | Sample collector + report generator |
| Telemetry | `src/metrics.rs` (`#696` block) | Publishes `soroban_pulse_slo_*` gauges |
| HTTP API | `GET /v1/admin/slo/report` | Returns the canonical report as JSON |
| HTTP API | `POST /v1/admin/slo/sample` | Records a sample (admin/integration) |
| Visualization | `docs/sli-slo-dashboard.json` | Grafana dashboard panels |
| Alerting | `docs/alerts.yml` (`SLO*`) | Prometheus burn-rate / budget alerts |

The tracker runs as a tokio task spawned from `main.rs` every
`SLO_DEFAULT_EVAL_INTERVAL_SECS` seconds (default: 60 s). It recomputes the
report for every registered SLO and republishes the gauges used by the
dashboard panels. Samples are appended by application code via
`crate::slo_tracker::record_sli_sample(&tracker, slo_name, value)`.

## SLO Definitions

The defaults are produced by `slo_tracker::default_slo_definitions()` and are
aligned with the latency tiers in [`docs/api-sla.md`](api-sla.md) and the
PromQL used by `docs/alerts.yml`.

| SLO | Component | Type | Target | Window |
|---|---|---|---|---|
| `http_availability` | http | ErrorRate | 99% (rate of non-5xx) | 24 h |
| `http_p99_latency` | http | Latency | ≤ 0.25 s (250 ms) | 24 h |
| `indexer_lag` | indexer | ThroughputSaturation | ≤ 100 ledgers | 1 h |
| `webhook_delivery_success` | webhook | ErrorRate | ≥ 95% | 1 h |
| `notification_delivery_latency` | notification | Latency | ≤ 30 s | 1 h |
| `replica_replay_lag` | replica | ThroughputSaturation | ≤ 30 s | 1 h |

Operators can register additional SLOs at startup via
`SloTracker::register(...)` with a custom `SloDefinition` (see
`src/slo_tracker.rs`).

## Prometheus metrics

| Metric | Type | Labels | Description |
|---|---|---|---|
| `soroban_pulse_slo_completion_ratio` | Gauge | `slo`, `component` | Fraction of good samples in the window, in `[0.0, 1.0]`. Drives the **SLO completion gauge**. |
| `soroban_pulse_slo_error_budget_remaining` | Gauge | `slo`, `component` | Fraction of the error budget still available. `1.0` = full, `0.0` = empty. |
| `soroban_pulse_slo_error_budget_consumed` | Gauge | `slo`, `component` | Fraction of the error budget already spent. |
| `soroban_pulse_slo_burn_rate` | Gauge | `slo`, `component` | Budget burn rate. ≈ 1 = on pace, > 2 = critical. |
| `soroban_pulse_sli_current_value` | Gauge | `slo`, `component` | Most recent SLI observation for an SLO. Drives the **SLI trend line chart**. |
| `soroban_pulse_slo_evaluation_total` | Counter | `slo`, `status` | Increments on each non-`Met` evaluation; `status` ∈ `at_risk`, `breached`. |
| `soroban_pulse_slo_tracked_count` | Gauge | — | Total SLOs registered with the tracker. |
| `soroban_pulse_slo_met_count` | Gauge | — | SLOs currently classified `met`. |
| `soroban_pulse_slo_at_risk_count` | Gauge | — | SLOs currently classified `at_risk`. |
| `soroban_pulse_slo_breached_count` | Gauge | — | SLOs currently classified `breached`. |

### Completion-gauge panel (PromQL)

```promql
soroban_pulse_slo_completion_ratio
```

The gauge is grouped by `slo` so each `Stat` / `Gauge` panel renders one dial
per registered SLO. Threshold bands:

| Band | Completion ratio | Visual |
|---|---|---|
| Met | ≥ `target` | Green |
| At risk | `target − 0.02 ≤ ratio < target` | Yellow |
| Breached | `< target − 0.02` *or* burn rate ≥ 2 | Red |

### SLI trend-line chart (PromQL)

```promql
soroban_pulse_sli_current_value
```

Series are grouped by `slo` so each panel renders one trend line per SLO.
A secondary Y-axis overlay can show the SLO target via
`on() group_left() soroban_pulse_slo_completion_ratio == 1` (or render the
target in the panel legend manually):

```promql
# Render the latest SLI value per SLO; the panel legend uses `{{slo}}` and the
# SLO target is overlaid as a constant line per panel (set under "Graph
# styles → Thresholds" in Grafana, or via a separate target label).
```promql
sum by (slo) (soroban_pulse_sli_current_value)
```
```

### Burn-rate overlay (PromQL)

```promql
max_over_time(soroban_pulse_slo_burn_rate[1h])
```

Useful for correlating dashboard dips with budget consumption.

## Prometheus alerts

Defined in `docs/alerts.yml` under the `# SLO error-budget alerts` block.

| Alert | Condition | Window | Severity |
|---|---|---|---|
| `SLOBudgetBurnRateFast` | `burn_rate > 14.4` | 2 min | critical (page) |
| `SLOBudgetBurnRateSlow` | `burn_rate > 6` | 5 min | warning |
| `SLOErrorBudgetLow` | `error_budget_remaining < 0.1` | 10 min | warning |
| `SLOCompletionBelowTarget` | `completion_ratio < 0.95` | 15 min | warning |
| `SLOSeverelyBreached` | `completion_ratio < 0.5` | 5 min | critical |

The `SLOBudgetBurnRateFast` / `SLOBudgetBurnRateSlow` pair implement the
"two-window" burn-rate alerts described in the Google SRE workbook:

* Fast burn: `burn_rate > 14.4` for 2 min ⇒ budget exhausted in 2 h.
* Slow burn: `burn_rate > 6` for 5 min ⇒ budget exhausted in 5 h.

## HTTP reporting endpoint

```bash
$ curl -sS -H "X-Api-Key: $ADMIN_API_KEY" \
       http://localhost:3000/v1/admin/slo/report | jq
```

```json
{
  "generated_at": 1750000000,
  "counts": { "met": 4, "at_risk": 1, "breached": 1 },
  "slos": [
    {
      "name": "http_availability",
      "description": "Fraction of HTTP responses that are not 5xx errors",
      "component": "http",
      "sli_type": "error_rate",
      "target": 0.99,
      "window_secs": 86400,
      "completion_ratio": 0.9998,
      "error_budget_consumed": 0.02,
      "error_budget_remaining": 0.98,
      "burn_rate": 0.02,
      "status": "met",
      "sample_count": 10000,
      "good_count":  9998,
      "last_sli_value": 1.0
    }
  ]
}
```

The same data is also exposed programmatically through
`crate::slo_tracker::current()` for internal callers.

## Grafana dashboard

Import `docs/sli-slo-dashboard.json` alongside `docs/grafana-dashboard.json`:

```bash
curl -X POST http://localhost:3000/api/dashboards/import \
  -H "Content-Type: application/json" \
  -d @docs/sli-slo-dashboard.json
```

The dashboard contains four rows:

1. **Overview** – Total SLOs, count by status, list of breached SLOs.
2. **Completion gauges** – One gauge panel per SLO with the threshold bands
   described above.
3. **SLI trend lines** – One time-series panel per SLO showing the latest SLI
   value with a horizontal target line.
4. **Burn-rate heatmap** – Heatmap of `burn_rate` per SLO so regressions are
   visible at a glance.

## SLO reporting

`GET /v1/admin/slo/report` is the single source of truth for SLOs. The
backend JSON report, the Prometheus metrics, and the Grafana panels all
derive from the same `Arc<RwLock<SloTracker>>` so they cannot diverge. For
the managed deployment, the monthly SLA report referenced in
[`docs/api-sla.md`](api-sla.md#reporting-and-incident-response) is generated
by aggregating the per-SLO reports over a calendar month.

To produce an out-of-band report for an incident, follow this runbook:

1. `curl /v1/admin/slo/report` immediately after the incident begins to
   capture the pre-incident state.
2. Snapshot the report every 5 min during the incident.
3. Diff consecutive snapshots to identify the SLOs whose completion ratio
   dropped and whose burn rate exceeded 1.
4. After the incident, identify the SLOs that breached and trigger the
   post-mortem runbook referenced in [`docs/api-sla.md`](api-sla.md).

## Enhanced Dashboard Panels (Issue #896)

The SLI/SLO dashboard includes several advanced panels for real-time monitoring:

### Latency Percentiles
Shows HTTP request latency at p50, p95, and p99 percentiles calculated from Prometheus histogram buckets.

**PromQL:**
```promql
histogram_quantile(0.95, rate(soroban_pulse_http_request_duration_seconds_bucket[5m]))
```

### Error Rate by Status Code
Tracks the percentage of 5xx errors relative to total requests, broken down by HTTP method and endpoint.

**PromQL:**
```promql
rate(soroban_pulse_http_request_duration_seconds_count{status_code=~"5.."}[5m]) 
/ 
rate(soroban_pulse_http_request_duration_seconds_count[5m])
```

### API Availability
Inverse of error rate — percentage of requests that succeeded (non-5xx responses).

**PromQL:**
```promql
1 - (rate(soroban_pulse_http_request_duration_seconds_count{status_code=~"5.."}[5m]) 
/ 
rate(soroban_pulse_http_request_duration_seconds_count[5m]))
```

### SLO Budget Burndown
Shows cumulative consumption of error budget over the window, useful for visualizing budget depletion trends.

**PromQL:**
```promql
1 - soroban_pulse_slo_error_budget_remaining
```

### Request Distribution by Endpoint
Histogram of request rates grouped by HTTP method and path, identifying which endpoints drive the most traffic.

**PromQL:**
```promql
sum(rate(soroban_pulse_http_request_duration_seconds_count[5m])) by (method, path)
```

## SLI Metric Calculations

SLI metrics are calculated in-process by `src/slo_tracker.rs`:

1. **Latency SLI**: Sample value is request duration in seconds. Good if `duration <= target`.
2. **Error Rate SLI**: Sample value is success count (1.0 = success, 0.0 = failure). Good if `value == 1.0`.
3. **Availability SLI**: Treated as error rate; `1.0` = up, `0.0` = down.
4. **Throughput/Saturation SLI**: Sample value is rate (requests/sec). Good if `rate <= target`.

Completion ratio is the fraction of good samples in the rolling window.

## Related documentation

* [API response time SLA](api-sla.md)
* [Grafana main dashboard](grafana-dashboard.json)
* [Alert rules](alerts.yml)
* [Capacity planning](capacity-planning.md)
* [Replica sync monitoring](replica-monitoring.md)
* [Performance regression testing](performance-regression-testing.md)
* [Distributed tracing configuration](tracing.md)
