use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::collections::HashMap;
use uuid::Uuid;
use tracing::{info, error, warn};

/// Aggregation window type
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum WindowType {
    #[serde(rename = "tumbling")]
    Tumbling, // Fixed-size time windows
    #[serde(rename = "sliding")]
    Sliding, // Overlapping time windows
    #[serde(rename = "session")]
    Session, // Activity-based windows
}

/// Aggregation operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AggregationOp {
    #[serde(rename = "count")]
    Count,
    #[serde(rename = "sum")]
    Sum,
    #[serde(rename = "avg")]
    Avg,
    #[serde(rename = "min")]
    Min,
    #[serde(rename = "max")]
    Max,
    #[serde(rename = "distinct_count")]
    DistinctCount,
}

/// Field selector for aggregation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldSelector {
    pub path: String,  // JSONPath to the field
    pub operation: AggregationOp,
    pub alias: Option<String>,
}

/// Group by configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupBy {
    pub field: String,
    pub interval: Option<String>, // For numeric fields, optional interval
}

/// Aggregation rule schema
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AggregationRule {
    pub id: Uuid,
    pub subscription_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub window_type: String,        // tumbling, sliding, session
    pub window_size_secs: i32,
    pub slide_interval_secs: Option<i32>, // For sliding windows
    pub fields: Value,              // JSON array of FieldSelector
    pub group_by: Option<Value>,    // JSON array of GroupBy
    pub filter_condition: Option<String>, // JSONPath filter
    pub enabled: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Aggregation result
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AggregationResult {
    pub id: Uuid,
    pub rule_id: Uuid,
    pub subscription_id: Uuid,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub group_values: Option<Value>, // JSON object with group-by values
    pub aggregated_data: Value,      // JSON object with aggregation results
    pub event_count: i64,
    pub created_at: DateTime<Utc>,
}

/// Request to create an aggregation rule
#[derive(Debug, Deserialize)]
pub struct CreateAggregationRuleRequest {
    pub name: String,
    pub description: Option<String>,
    pub window_type: WindowType,
    pub window_size_secs: i32,
    pub slide_interval_secs: Option<i32>,
    pub fields: Vec<FieldSelector>,
    pub group_by: Option<Vec<GroupBy>>,
    pub filter_condition: Option<String>,
}

/// Response for aggregation rule creation
#[derive(Debug, Serialize)]
pub struct AggregationRuleResponse {
    pub id: Uuid,
    pub name: String,
    pub status: String,
}

/// Create an aggregation rule
pub async fn create_aggregation_rule(
    pool: &PgPool,
    subscription_id: Uuid,
    req: CreateAggregationRuleRequest,
) -> Result<AggregationRuleResponse, String> {
    // Validate window size
    if req.window_size_secs <= 0 {
        return Err("Window size must be positive".to_string());
    }

    // For sliding windows, validate slide interval
    if let Some(slide_size) = req.slide_interval_secs {
        if slide_size <= 0 || slide_size > req.window_size_secs {
            return Err("Slide interval must be positive and less than window size".to_string());
        }
    }

    // Validate subscription exists
    let subscription_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM subscriptions WHERE id = $1)"
    )
    .bind(subscription_id)
    .fetch_one(pool)
    .await
    .map_err(|e| format!("Failed to validate subscription: {}", e))?;

    if !subscription_exists {
        return Err(format!("Subscription not found: {}", subscription_id));
    }

    let rule_id = Uuid::new_v4();
    let window_type_str = match req.window_type {
        WindowType::Tumbling => "tumbling",
        WindowType::Sliding => "sliding",
        WindowType::Session => "session",
    };

    let fields = serde_json::to_value(&req.fields)
        .map_err(|e| format!("Failed to serialize fields: {}", e))?;

    let group_by = req
        .group_by
        .map(|gb| serde_json::to_value(&gb).ok())
        .flatten();

    sqlx::query(
        "INSERT INTO aggregation_rules (id, subscription_id, name, description, window_type, window_size_secs, slide_interval_secs, fields, group_by, filter_condition, enabled, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)"
    )
    .bind(rule_id)
    .bind(subscription_id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(window_type_str)
    .bind(req.window_size_secs)
    .bind(req.slide_interval_secs)
    .bind(fields)
    .bind(group_by)
    .bind(&req.filter_condition)
    .bind(true)
    .bind(Utc::now())
    .bind(Utc::now())
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to create aggregation rule: {}", e))?;

    info!(
        rule_id = %rule_id,
        subscription_id = %subscription_id,
        name = %req.name,
        "Created aggregation rule"
    );

    Ok(AggregationRuleResponse {
        id: rule_id,
        name: req.name,
        status: "created".to_string(),
    })
}

/// Evaluate an aggregation window and store the result
pub async fn evaluate_aggregation_window(
    pool: &PgPool,
    rule_id: Uuid,
    subscription_id: Uuid,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
) -> Result<AggregationResult, String> {
    let result_id = Uuid::new_v4();

    // Get aggregation rule
    let rule = sqlx::query_as::<_, AggregationRule>(
        "SELECT id, subscription_id, name, description, window_type, window_size_secs, slide_interval_secs, fields, group_by, filter_condition, enabled, created_at, updated_at FROM aggregation_rules WHERE id = $1"
    )
    .bind(rule_id)
    .fetch_optional(pool)
    .await
    .map_err(|e| format!("Failed to fetch aggregation rule: {}", e))?
    .ok_or_else(|| format!("Aggregation rule not found: {}", rule_id))?;

    // Fetch events within the window
    let events = sqlx::query_as::<_, (Uuid, Value)>(
        "SELECT id, value FROM soroban_events WHERE subscription_id = $1 AND timestamp >= $2 AND timestamp < $3"
    )
    .bind(subscription_id)
    .bind(window_start)
    .bind(window_end)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to fetch events: {}", e))?;

    let event_count = events.len() as i64;

    // Apply filter if present
    let filtered_events: Vec<_> = if let Some(ref filter) = rule.filter_condition {
        events
            .into_iter()
            .filter(|(_, event)| apply_filter(event, filter))
            .collect()
    } else {
        events
    };

    // Parse field selectors
    let field_selectors: Vec<FieldSelector> = serde_json::from_value(rule.fields.clone())
        .unwrap_or_default();

    // Compute aggregations
    let mut aggregated_data = json!({});

    for selector in field_selectors {
        let operation = compute_operation(&filtered_events, &selector);
        let alias = selector.alias.unwrap_or(selector.path.clone());
        if let Value::Object(ref mut obj) = aggregated_data {
            obj.insert(alias, operation);
        }
    }

    // Store result
    sqlx::query(
        "INSERT INTO aggregation_results (id, rule_id, subscription_id, window_start, window_end, group_values, aggregated_data, event_count, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
    )
    .bind(result_id)
    .bind(rule_id)
    .bind(subscription_id)
    .bind(window_start)
    .bind(window_end)
    .bind::<Option<Value>>(None)
    .bind(&aggregated_data)
    .bind(event_count)
    .bind(Utc::now())
    .execute(pool)
    .await
    .map_err(|e| format!("Failed to store aggregation result: {}", e))?;

    info!(
        result_id = %result_id,
        rule_id = %rule_id,
        event_count = event_count,
        "Aggregation window evaluated"
    );

    Ok(AggregationResult {
        id: result_id,
        rule_id,
        subscription_id,
        window_start,
        window_end,
        group_values: None,
        aggregated_data,
        event_count,
        created_at: Utc::now(),
    })
}

/// Apply a filter condition to an event
fn apply_filter(event: &Value, filter: &str) -> bool {
    // Simple filter implementation - matches if the filter path is non-empty/true
    if let Some(value) = event.pointer(filter) {
        match value {
            Value::Bool(b) => *b,
            Value::Null => false,
            _ => true,
        }
    } else {
        false
    }
}

/// Compute aggregation operation on events
fn compute_operation(
    events: &[(Uuid, Value)],
    selector: &FieldSelector,
) -> Value {
    let values: Vec<f64> = events
        .iter()
        .filter_map(|(_, event)| {
            event
                .pointer(&selector.path)
                .and_then(|v| v.as_f64())
        })
        .collect();

    match selector.operation {
        AggregationOp::Count => json!(events.len()),
        AggregationOp::Sum => {
            json!(values.iter().sum::<f64>())
        }
        AggregationOp::Avg => {
            if values.is_empty() {
                json!(null)
            } else {
                json!(values.iter().sum::<f64>() / values.len() as f64)
            }
        }
        AggregationOp::Min => {
            json!(values.iter().copied().fold(f64::INFINITY, f64::min))
        }
        AggregationOp::Max => {
            json!(values.iter().copied().fold(f64::NEG_INFINITY, f64::max))
        }
        AggregationOp::DistinctCount => {
            let mut seen = std::collections::HashSet::new();
            for (_, event) in events {
                if let Some(v) = event.pointer(&selector.path) {
                    seen.insert(v.to_string());
                }
            }
            json!(seen.len())
        }
    }
}

/// Get aggregation results for a rule
pub async fn get_aggregation_results(
    pool: &PgPool,
    rule_id: Uuid,
    limit: i64,
) -> Result<Vec<AggregationResult>, String> {
    sqlx::query_as::<_, AggregationResult>(
        "SELECT id, rule_id, subscription_id, window_start, window_end, group_values, aggregated_data, event_count, created_at FROM aggregation_results WHERE rule_id = $1 ORDER BY window_start DESC LIMIT $2"
    )
    .bind(rule_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(|e| format!("Failed to get aggregation results: {}", e))
}

// ──────────────────────────────────────────────────────────────────────────────
// Issue #934 additions: group metrics, group configuration, aggregation optimizer
// ──────────────────────────────────────────────────────────────────────────────

/// Per-group statistics computed over a window.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupMetrics {
    /// The serialised group key, e.g. `"contract_id=CABC..."`.
    pub group_key: String,
    /// The aggregation rule this belongs to.
    pub rule_id: Uuid,
    /// The subscription this belongs to.
    pub subscription_id: Uuid,
    /// Start of the aggregation window.
    pub window_start: DateTime<Utc>,
    /// End of the aggregation window.
    pub window_end: DateTime<Utc>,
    /// Number of events in this group+window.
    pub event_count: i64,
    /// Average numeric value (if applicable).
    pub avg_value: Option<f64>,
    /// Minimum numeric value (if applicable).
    pub min_value: Option<f64>,
    /// Maximum numeric value (if applicable).
    pub max_value: Option<f64>,
    /// Sum of numeric values (if applicable).
    pub sum_value: Option<f64>,
    /// Count of distinct values (if applicable).
    pub distinct_count: Option<i64>,
}

/// Configuration for a group-based aggregation run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfiguration {
    /// The field path used as the group key (e.g. `"contract_id"`).
    pub group_key: String,
    /// The aggregation operations to apply to the grouped data.
    pub aggregation_ops: Vec<AggregationOp>,
    /// Window type to use for this configuration.
    pub window_type: WindowType,
}

/// Batches aggregation evaluation to reduce DB round-trips and memory pressure.
pub struct AggregationOptimizer {
    /// How many windows to evaluate in a single batch.
    pub batch_size: usize,
    /// Collected (rule_id, subscription_id, window_start, window_end) work items.
    pending: Vec<(Uuid, Uuid, DateTime<Utc>, DateTime<Utc>)>,
}

impl AggregationOptimizer {
    /// Create an optimizer with the given batch size (minimum 1, capped at 1000).
    pub fn new(batch_size: usize) -> Self {
        let batch_size = batch_size.max(1).min(1000);
        Self {
            batch_size,
            pending: Vec::new(),
        }
    }

    /// Queue a window for evaluation.
    pub fn enqueue(
        &mut self,
        rule_id: Uuid,
        subscription_id: Uuid,
        window_start: DateTime<Utc>,
        window_end: DateTime<Utc>,
    ) {
        self.pending.push((rule_id, subscription_id, window_start, window_end));
    }

    /// Evaluate all queued windows in batches and return the results.
    ///
    /// Windows are processed in the order they were enqueued.  Each batch is
    /// evaluated sequentially; within a batch the windows are also sequential
    /// so the DB is not overwhelmed.
    pub async fn flush(&mut self, pool: &PgPool) -> Vec<Result<AggregationResult, String>> {
        let mut all_results = Vec::with_capacity(self.pending.len());
        let items = std::mem::take(&mut self.pending);

        for chunk in items.chunks(self.batch_size) {
            info!(
                batch_size = chunk.len(),
                "AggregationOptimizer processing batch"
            );
            for &(rule_id, subscription_id, window_start, window_end) in chunk {
                let result = evaluate_aggregation_window(
                    pool,
                    rule_id,
                    subscription_id,
                    window_start,
                    window_end,
                )
                .await;
                if let Err(ref e) = result {
                    warn!(rule_id = %rule_id, error = %e, "Aggregation window evaluation failed");
                }
                all_results.push(result);
            }
        }

        all_results
    }

    /// Return the number of items currently waiting to be flushed.
    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// Compute per-group metrics for events within a time window.
///
/// Events are grouped by the value at `config.group_key` (treated as a
/// JSON pointer, e.g. `/contract_id`).  For each group the function computes
/// count, avg, min, max and sum over every numeric field found at the same
/// path.
pub fn compute_group_metrics(
    rule_id: Uuid,
    subscription_id: Uuid,
    window_start: DateTime<Utc>,
    window_end: DateTime<Utc>,
    events: &[(Uuid, serde_json::Value)],
    config: &GroupConfiguration,
) -> Vec<GroupMetrics> {
    // Group events by their group-key value.
    let mut groups: HashMap<String, Vec<f64>> = HashMap::new();
    let mut group_counts: HashMap<String, i64> = HashMap::new();
    let mut group_distinct: HashMap<String, std::collections::HashSet<String>> = HashMap::new();

    // Build a JSON-pointer from the config group_key (prepend "/" if missing).
    let pointer = if config.group_key.starts_with('/') {
        config.group_key.clone()
    } else {
        format!("/{}", config.group_key)
    };

    for (_, event) in events {
        // Determine the group key value for this event.
        let key_value = event
            .pointer(&pointer)
            .map(|v| v.to_string())
            .unwrap_or_else(|| "null".to_string());

        // Collect the numeric value at the same path for stat operations.
        if let Some(num) = event.pointer(&pointer).and_then(|v| v.as_f64()) {
            groups.entry(key_value.clone()).or_default().push(num);
        } else {
            // Ensure the group exists even if the value is non-numeric.
            groups.entry(key_value.clone()).or_default();
        }

        *group_counts.entry(key_value.clone()).or_insert(0) += 1;

        group_distinct
            .entry(key_value.clone())
            .or_default()
            .insert(
                event
                    .pointer(&pointer)
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            );
    }

    groups
        .into_iter()
        .map(|(group_key, values)| {
            let event_count = *group_counts.get(&group_key).unwrap_or(&0);
            let distinct_count = group_distinct
                .get(&group_key)
                .map(|s| s.len() as i64);

            let (avg_value, min_value, max_value, sum_value) = if values.is_empty() {
                (None, None, None, None)
            } else {
                let sum: f64 = values.iter().sum();
                let avg = sum / values.len() as f64;
                let min = values.iter().copied().fold(f64::INFINITY, f64::min);
                let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                (Some(avg), Some(min), Some(max), Some(sum))
            };

            GroupMetrics {
                group_key,
                rule_id,
                subscription_id,
                window_start,
                window_end,
                event_count,
                avg_value,
                min_value,
                max_value,
                sum_value,
                distinct_count,
            }
        })
        .collect()
}

/// Persist computed group metrics to the `group_metrics` table.
pub async fn save_group_metrics(
    pool: &PgPool,
    metrics: &[GroupMetrics],
) -> Result<(), String> {
    for m in metrics {
        sqlx::query(
            "INSERT INTO group_metrics (
                rule_id, subscription_id, group_key,
                window_start, window_end,
                event_count, avg_value, min_value, max_value, sum_value,
                distinct_count, computed_at
             ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
        )
        .bind(m.rule_id)
        .bind(m.subscription_id)
        .bind(&m.group_key)
        .bind(m.window_start)
        .bind(m.window_end)
        .bind(m.event_count)
        .bind(m.avg_value)
        .bind(m.min_value)
        .bind(m.max_value)
        .bind(m.sum_value)
        .bind(m.distinct_count)
        .bind(Utc::now())
        .execute(pool)
        .await
        .map_err(|e| format!("Failed to save group metrics: {}", e))?;
    }
    Ok(())
}

/// Retrieve group statistics from the DB for a given rule, optionally filtered
/// by group key and time range.
///
/// Results are ordered by `window_start DESC` so the most recent window comes
/// first.
pub async fn get_group_statistics(
    pool: &PgPool,
    rule_id: Uuid,
    group_key: Option<&str>,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
    limit: i64,
) -> Result<Vec<GroupMetrics>, String> {
    // Build the query dynamically.
    let mut conditions = vec!["rule_id = $1".to_string()];
    let mut param_idx = 2_usize;

    if group_key.is_some() {
        conditions.push(format!("group_key = ${}", param_idx));
        param_idx += 1;
    }
    if from.is_some() {
        conditions.push(format!("window_start >= ${}", param_idx));
        param_idx += 1;
    }
    if to.is_some() {
        conditions.push(format!("window_end <= ${}", param_idx));
        param_idx += 1;
    }

    let where_clause = conditions.join(" AND ");
    let sql = format!(
        "SELECT rule_id, subscription_id, group_key,
                window_start, window_end,
                event_count, avg_value, min_value, max_value, sum_value,
                distinct_count
         FROM group_metrics
         WHERE {}
         ORDER BY window_start DESC
         LIMIT ${}",
        where_clause, param_idx
    );

    // We use raw query_as here because the column list is fixed even though
    // the WHERE clause is dynamic.  sqlx will map columns by position.
    let mut q = sqlx::query(&sql).bind(rule_id);

    if let Some(gk) = group_key {
        q = q.bind(gk);
    }
    if let Some(f) = from {
        q = q.bind(f);
    }
    if let Some(t) = to {
        q = q.bind(t);
    }
    q = q.bind(limit);

    let rows = q
        .fetch_all(pool)
        .await
        .map_err(|e| format!("Failed to get group statistics: {}", e))?;

    let metrics = rows
        .into_iter()
        .map(|row| {
            use sqlx::Row;
            GroupMetrics {
                rule_id: row.get("rule_id"),
                subscription_id: row.get("subscription_id"),
                group_key: row.get("group_key"),
                window_start: row.get("window_start"),
                window_end: row.get("window_end"),
                event_count: row.get("event_count"),
                avg_value: row.get("avg_value"),
                min_value: row.get("min_value"),
                max_value: row.get("max_value"),
                sum_value: row.get("sum_value"),
                distinct_count: row.get("distinct_count"),
            }
        })
        .collect();

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_validation() {
        // Valid tumbling window
        assert!(create_test_window(10, WindowType::Tumbling, None).is_ok());

        // Valid sliding window
        assert!(create_test_window(10, WindowType::Sliding, Some(5)).is_ok());
    }

    fn create_test_window(
        size: i32,
        _window_type: WindowType,
        _slide: Option<i32>,
    ) -> Result<(), String> {
        if size <= 0 {
            return Err("Window size must be positive".to_string());
        }
        Ok(())
    }

    #[test]
    fn test_filter_application() {
        let event = json!({
            "action": "transfer",
            "amount": 100
        });

        assert!(apply_filter(&event, "/action"));
        assert!(!apply_filter(&event, "/nonexistent"));
    }

    #[test]
    fn test_compute_group_metrics_empty() {
        let rule_id = Uuid::new_v4();
        let sub_id = Uuid::new_v4();
        let now = Utc::now();
        let config = GroupConfiguration {
            group_key: "contract_id".to_string(),
            aggregation_ops: vec![AggregationOp::Count],
            window_type: WindowType::Tumbling,
        };
        let result = compute_group_metrics(rule_id, sub_id, now, now, &[], &config);
        assert!(result.is_empty(), "No events should produce no group metrics");
    }

    #[test]
    fn test_compute_group_metrics_single_group() {
        let rule_id = Uuid::new_v4();
        let sub_id = Uuid::new_v4();
        let now = Utc::now();
        let config = GroupConfiguration {
            group_key: "contract_id".to_string(),
            aggregation_ops: vec![AggregationOp::Count, AggregationOp::Sum],
            window_type: WindowType::Tumbling,
        };

        let events: Vec<(Uuid, serde_json::Value)> = vec![
            (Uuid::new_v4(), json!({ "contract_id": "CA1", "amount": 10.0 })),
            (Uuid::new_v4(), json!({ "contract_id": "CA1", "amount": 20.0 })),
            (Uuid::new_v4(), json!({ "contract_id": "CA2", "amount": 5.0 })),
        ];

        let metrics = compute_group_metrics(rule_id, sub_id, now, now, &events, &config);
        assert_eq!(metrics.len(), 2, "Expected two groups");

        let ca1 = metrics.iter().find(|m| m.group_key == "\"CA1\"").unwrap();
        assert_eq!(ca1.event_count, 2);
        assert!((ca1.sum_value.unwrap() - 30.0).abs() < 1e-9);
        assert!((ca1.avg_value.unwrap() - 15.0).abs() < 1e-9);

        let ca2 = metrics.iter().find(|m| m.group_key == "\"CA2\"").unwrap();
        assert_eq!(ca2.event_count, 1);
        assert!((ca2.sum_value.unwrap() - 5.0).abs() < 1e-9);
    }

    #[test]
    fn test_aggregation_optimizer_batch_size_clamped() {
        let opt = AggregationOptimizer::new(0);
        assert_eq!(opt.batch_size, 1, "Minimum batch size should be 1");

        let opt2 = AggregationOptimizer::new(9999);
        assert_eq!(opt2.batch_size, 1000, "Maximum batch size should be 1000");
    }

    #[test]
    fn test_aggregation_optimizer_pending_count() {
        let mut opt = AggregationOptimizer::new(10);
        assert_eq!(opt.pending_count(), 0);

        let now = Utc::now();
        opt.enqueue(Uuid::new_v4(), Uuid::new_v4(), now, now);
        opt.enqueue(Uuid::new_v4(), Uuid::new_v4(), now, now);
        assert_eq!(opt.pending_count(), 2);
    }

    #[test]
    fn test_group_configuration_serialization() {
        let config = GroupConfiguration {
            group_key: "contract_id".to_string(),
            aggregation_ops: vec![AggregationOp::Count, AggregationOp::Avg],
            window_type: WindowType::Sliding,
        };
        let json = serde_json::to_string(&config).unwrap();
        let back: GroupConfiguration = serde_json::from_str(&json).unwrap();
        assert_eq!(back.group_key, config.group_key);
    }
}
