# Time Series Analysis

SorobanPulse can produce time-bucketed event counts for any contract (or for
all contracts combined) and run statistical analyses on those series.  This
document covers the data model, the analysis functions, caching, and the
migration added for issue #932.

---

## Overview

The time series subsystem lives in `src/time_series.rs` and is exposed as the
`soroban_pulse::time_series` crate module.  It provides:

- **`get_time_series`** — query bucketed event counts from the live `events`
  table.
- **`detect_trend`** — linear-regression trend analysis.
- **`detect_seasonality`** — period detection via mean-absolute-deviation.
- **`detect_anomalies`** — mean ± k·σ outlier flagging.
- **`forecast_next_points`** — linear extrapolation of future counts.

A `event_time_series_cache` table (see [Migration](#migration)) can be
populated by a background job to pre-compute buckets and avoid repeating
expensive `GROUP BY` queries.

---

## Granularity

All time series functions operate at one of four granularities:

| Variant | SQL `date_trunc` unit | Bucket duration | Seasonal period |
|---------|-----------------------|-----------------|-----------------|
| `Hourly` | `hour` | 1 hour | 24 buckets (daily) |
| `Daily` | `day` | 1 day | 7 buckets (weekly) |
| `Weekly` | `week` | 1 week | 4 buckets (monthly) |
| `Monthly` | `month` | ~30 days | 12 buckets (annual) |

The granularity is specified in `TimeSeriesQuery.granularity` and is returned
in `TimeSeriesSeries.granularity` as a lowercase string.

---

## Data Types

### `TimeSeriesQuery`

Input to `get_time_series`.

| Field | Type | Description |
|-------|------|-------------|
| `contract_id` | `Option<String>` | Filter to one contract; `None` = all. |
| `from` | `Option<DateTime<Utc>>` | Inclusive range start. |
| `to` | `Option<DateTime<Utc>>` | Inclusive range end. |
| `granularity` | `TimeSeriesGranularity` | Bucketing granularity. |

### `TimeSeriesPoint`

One bucket in the series.

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | `DateTime<Utc>` | Start of the bucket. |
| `count` | `i64` | Events in this bucket. |
| `contract_id` | `Option<String>` | Contract (mirrors the query). |

### `TimeSeriesSeries`

| Field | Type | Description |
|-------|------|-------------|
| `points` | `Vec<TimeSeriesPoint>` | Ordered from oldest to newest. |
| `granularity` | `String` | Lowercase label, e.g. `"daily"`. |
| `total` | `i64` | Sum of all counts. |

### `TrendAnalysis`

| Field | Type | Description |
|-------|------|-------------|
| `direction` | `TrendDirection` | `increasing`, `decreasing` or `stable`. |
| `slope` | `f64` | Regression slope (events per bucket). |
| `confidence` | `f64` | R² [0, 1]; 1.0 = perfect linear fit. |
| `data_points` | `usize` | Number of points used. |

### `SeasonalityResult`

| Field | Type | Description |
|-------|------|-------------|
| `has_seasonality` | `bool` | `true` if strength ≥ 0.7. |
| `period_hours` | `Option<f64>` | Estimated period in hours. |
| `strength` | `f64` | Normalised signal strength [0, 1]. |

### `TimeSeriesAnomalies`

| Field | Type | Description |
|-------|------|-------------|
| `anomalies` | `Vec<AnomalyPoint>` | Points exceeding the threshold. |
| `threshold_multiplier` | `f64` | The k used for k·σ. |

### `AnomalyPoint`

| Field | Type | Description |
|-------|------|-------------|
| `timestamp` | `DateTime<Utc>` | Bucket timestamp. |
| `value` | `i64` | Actual count. |
| `expected` | `f64` | Mean of the whole series. |
| `deviation` | `f64` | `|value - mean| / stddev`. |

---

## Functions

### `get_time_series`

```rust
pub async fn get_time_series(
    pool: &PgPool,
    query: TimeSeriesQuery,
) -> Result<TimeSeriesSeries, String>
```

Issues a single SQL query:

```sql
SELECT date_trunc($1, timestamp) AS bucket, COUNT(*) AS count
FROM events
WHERE [contract_id = $2] [AND timestamp >= $3] [AND timestamp <= $4]
GROUP BY bucket
ORDER BY bucket ASC
```

`$1` is the `date_trunc` unit string (e.g. `"hour"`).  Optional WHERE clauses
are appended only when the corresponding query fields are set.

### `detect_trend`

```rust
pub fn detect_trend(series: &TimeSeriesSeries) -> TrendAnalysis
```

Fits a least-squares regression line through the points (x = bucket index,
y = count).  Requires at least 2 points; returns `Stable` with 0 slope
otherwise.

**Direction thresholds** (slope in events/bucket):

| Condition | Direction |
|-----------|-----------|
| slope > 0.05 | `Increasing` |
| slope < −0.05 | `Decreasing` |
| otherwise | `Stable` |

### `detect_seasonality`

```rust
pub fn detect_seasonality(series: &TimeSeriesSeries) -> SeasonalityResult
```

Compares each data point to the point one candidate period earlier.  The
candidate period is the typical natural period for the granularity (e.g. 24
buckets for hourly data).  Requires at least two full periods.

**Strength formula:**

```
strength = 1 - MAD / mean
```

where MAD is the mean absolute deviation between pairs separated by one period.
Strength is clamped to [0, 1]; values ≥ 0.7 are classified as seasonal.

### `detect_anomalies`

```rust
pub fn detect_anomalies(
    series: &TimeSeriesSeries,
    threshold_multiplier: f64,
) -> TimeSeriesAnomalies
```

Computes the global mean and standard deviation of `count` across all points.
A point is flagged when:

```
|count - mean| > threshold_multiplier × stddev
```

Requires at least 3 points.  Recommended starting value for
`threshold_multiplier`: `2.0` (≈ 95th percentile).  Use `3.0` for fewer
but more certain anomalies.

### `forecast_next_points`

```rust
pub fn forecast_next_points(
    series: &TimeSeriesSeries,
    count: usize,
) -> Vec<TimeSeriesPoint>
```

Extrapolates `count` future buckets using the same linear regression as
`detect_trend`.  Forecasted counts are floored at 0.  Timestamps advance by
exactly one bucket duration per step.

---

## Typical Usage

```rust
use soroban_pulse::time_series::{
    TimeSeriesGranularity, TimeSeriesQuery,
    get_time_series, detect_trend, detect_seasonality,
    detect_anomalies, forecast_next_points,
};
use chrono::Utc;

// 1. Fetch the last 30 days of daily event counts for one contract.
let query = TimeSeriesQuery {
    contract_id: Some("CABC...".to_string()),
    from: Some(Utc::now() - chrono::Duration::days(30)),
    to: Some(Utc::now()),
    granularity: TimeSeriesGranularity::Daily,
};
let series = get_time_series(&pool, query).await?;

// 2. Analyse the trend.
let trend = detect_trend(&series);
println!("Trend: {:?}, slope={:.2}", trend.direction, trend.slope);

// 3. Check for seasonality.
let seasonality = detect_seasonality(&series);
if seasonality.has_seasonality {
    println!("Weekly pattern detected (strength={:.2})", seasonality.strength);
}

// 4. Flag anomalies (2σ threshold).
let anomalies = detect_anomalies(&series, 2.0);
for a in &anomalies.anomalies {
    println!("Anomaly at {}: {} events ({:.1}σ)", a.timestamp, a.value, a.deviation);
}

// 5. Forecast the next 7 days.
let forecast = forecast_next_points(&series, 7);
for p in forecast {
    println!("Forecast {}: {} events", p.timestamp.date_naive(), p.count);
}
```

---

## Migration

Migration file: `migrations/20260830000002_event_time_series.sql`

```sql
CREATE TABLE IF NOT EXISTS event_time_series_cache (
    id           UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    contract_id  TEXT,
    granularity  TEXT        NOT NULL,
    bucket_start TIMESTAMPTZ NOT NULL,
    bucket_end   TIMESTAMPTZ NOT NULL,
    event_count  BIGINT      NOT NULL DEFAULT 0,
    computed_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE (contract_id, granularity, bucket_start)
);

CREATE INDEX IF NOT EXISTS idx_ts_cache_contract_granularity_start
    ON event_time_series_cache(contract_id, granularity, bucket_start);

CREATE INDEX IF NOT EXISTS idx_ts_cache_granularity_start
    ON event_time_series_cache(granularity, bucket_start);

CREATE INDEX IF NOT EXISTS idx_ts_cache_computed_at
    ON event_time_series_cache(computed_at);
```

### Cache table columns

| Column | Type | Description |
|--------|------|-------------|
| `contract_id` | TEXT | `NULL` for all-contracts rollups. |
| `granularity` | TEXT | `hourly`, `daily`, `weekly`, or `monthly`. |
| `bucket_start` | TIMESTAMPTZ | Start of the time bucket. |
| `bucket_end` | TIMESTAMPTZ | End of the time bucket. |
| `event_count` | BIGINT | Pre-computed count. |
| `computed_at` | TIMESTAMPTZ | When the cache row was last written. |

The `UNIQUE(contract_id, granularity, bucket_start)` constraint lets a
background job use `INSERT … ON CONFLICT DO UPDATE SET event_count = …` to
refresh stale buckets without duplicating rows.

---

## Caching Strategy

The cache table stores pre-computed buckets.  The recommended refresh approach:

1. A scheduled task (e.g. every 5 minutes for hourly buckets, every hour for
   daily buckets) queries the `events` table with `date_trunc` and upserts the
   result into `event_time_series_cache`.
2. Read-heavy API endpoints check the cache first; if the most recent
   `computed_at` is within an acceptable staleness window, they return the
   cached rows directly.  Otherwise they fall back to a live query.

This avoids running a full `GROUP BY` scan on large `events` tables for every
API request.

---

## Performance Considerations

- **Index coverage**: `events(contract_id, timestamp)` (created by earlier
  migrations) covers the primary filter path.  The `date_trunc` expression is
  not indexable, so the query always performs a range scan followed by a hash
  aggregate.
- **Large datasets**: For tenants with millions of events per day, populate the
  cache table aggressively and query from it rather than from `events` directly.
- **Memory**: `detect_anomalies`, `detect_trend`, and `forecast_next_points`
  operate entirely in Rust memory on the `Vec<TimeSeriesPoint>` already
  returned from the DB.  No additional DB queries are issued after
  `get_time_series` returns.
- **Forecasting accuracy**: The linear model works well for steadily growing or
  shrinking series but will be inaccurate for highly seasonal or cyclic data.
  For seasonal series, consider subtracting the seasonal component before
  forecasting and adding it back afterwards.
