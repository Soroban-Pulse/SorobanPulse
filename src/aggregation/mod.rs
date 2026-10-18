//! Event aggregation.
//!
//! Two entry points share one operation type ([`AggregationOp`]) and one
//! numeric reducer ([`NumericSummary`]):
//!
//! - **On-demand queries** ([`aggregate_events`]): group indexed events by an
//!   allow-listed JSONB path and aggregate them in SQL.
//! - **Windowed rules** ([`create_aggregation_rule`],
//!   [`evaluate_aggregation_window`], [`compute_group_metrics`], …):
//!   per-subscription rules evaluated over tumbling / sliding / session
//!   windows, with results persisted for later retrieval.

use axum::{
    extract::{Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::collections::{HashMap, HashSet};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{error::AppError, routes::AppState};

// ──────────────────────────────────────────────────────────────────────────────
// Shared building blocks
// ──────────────────────────────────────────────────────────────────────────────

/// Aggregation operation, used by both on-demand queries and windowed rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AggregationOp {
    Count,
    Sum,
    Avg,
    Min,
    Max,
    DistinctCount,
}

/// Name used by the on-demand query API (`agg_fn`).
pub type AggregationFunction = AggregationOp;

impl AggregationOp {
    /// The wire / SQL-facing name of the operation.
    pub fn as_str(self) -> &'static str {
        match self {
            AggregationOp::Count => "count",
            AggregationOp::Sum => "sum",
            AggregationOp::Avg => "avg",
            AggregationOp::Min => "min",
            AggregationOp::Max => "max",
            AggregationOp::DistinctCount => "distinct_count",
        }
    }

    /// SQL aggregate expression over `event_data`, cast to `float8` so every
    /// operation decodes as `f64`. `value_path` is required for every
    /// operation except `count`, and must already be validated.
    fn sql_expr(self, value_path: Option<&str>) -> Result<String, AppError> {
        let field = || {
            value_path
                .map(|p| format!("(event_data->>'{}')", p.trim_start_matches("$.")))
                .ok_or_else(|| {
                    AppError::Validation(format!(
                        "value_path required for {} aggregation",
                        self.as_str()
                    ))
                })
        };
        let expr = match self {
            AggregationOp::Count => "COUNT(*)".to_string(),
            AggregationOp::Sum => format!("SUM({}::numeric)", field()?),
            AggregationOp::Avg => format!("AVG({}::numeric)", field()?),
            AggregationOp::Min => format!("MIN({}::numeric)", field()?),
            AggregationOp::Max => format!("MAX({}::numeric)", field()?),
            AggregationOp::DistinctCount => format!("COUNT(DISTINCT {})", field()?),
        };
        Ok(format!("({})::float8", expr))
    }
}

/// Count / sum / min / max over a set of numeric values.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct NumericSummary {
    pub count: usize,
    pub sum: f64,
    pub min: Option<f64>,
    pub max: Option<f64>,
}

impl NumericSummary {
    pub fn from_values<I: IntoIterator<Item = f64>>(values: I) -> Self {
        values.into_iter().fold(Self::default(), |s, v| Self {
            count: s.count + 1,
            sum: s.sum + v,
            min: Some(s.min.map_or(v, |m| m.min(v))),
            max: Some(s.max.map_or(v, |m| m.max(v))),
        })
    }

    /// Arithmetic mean, or `None` when there are no values.
    pub fn avg(&self) -> Option<f64> {
        (self.count > 0).then(|| self.sum / self.count as f64)
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// On-demand aggregation queries
// ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AggregationParams {
    pub group_by: String,
    pub agg_fn: AggregationFunction,
    pub value_path: Option<String>,
    pub contract_id: Option<String>,
    pub contract_ids: Option<String>,
    pub from_ledger: Option<i64>,
    pub to_ledger: Option<i64>,
    pub limit: Option<i64>,
}

/// One group in an on-demand aggregation response.
#[derive(Debug, Serialize)]
pub struct GroupAggregationResult {
    pub group_value: Value,
    pub result: f64,
}

#[derive(Debug, Serialize)]
pub struct AggregationResponse {
    pub results: Vec<GroupAggregationResult>,
    pub total_groups: usize,
    pub aggregation_function: AggregationFunction,
    pub group_by_path: String,
}

const MAX_AGGREGATION_GROUPS: i64 = 1000;
const ALLOWED_JSONB_PATHS: &[&str] = &[
    "$.token",
    "$.amount",
    "$.from",
    "$.to",
    "$.topic",
    "$.type",
    "$.status",
];

pub async fn aggregate_events(
    State(state): State<AppState>,
    Query(params): Query<AggregationParams>,
) -> Result<Json<AggregationResponse>, AppError> {
    // Validate JSONB path to prevent injection
    validate_jsonb_path(&params.group_by)?;

    if let Some(ref value_path) = params.value_path {
        validate_jsonb_path(value_path)?;
    }

    // Build the aggregation query
    let (query, bind_values) = build_aggregation_query(&params)?;

    // Bind parameters in order
    let mut bound_query = sqlx::query(&query);
    for value in bind_values {
        bound_query = match value {
            BindValue::String(s) => bound_query.bind(s),
            BindValue::I64(i) => bound_query.bind(i),
        };
    }

    let results = bound_query
        .fetch_all(&state.pool)
        .await
        .map_err(|e| AppError::Internal(format!("Aggregation query failed: {}", e)))?;

    // Parse results
    let mut aggregation_results = Vec::new();
    for row in results {
        let group_value: Value = row.try_get("group_value")
            .map_err(|e| AppError::Internal(format!("Failed to get group_value: {}", e)))?;
        let result: f64 = row.try_get("agg_result")
            .map_err(|e| AppError::Internal(format!("Failed to get agg_result: {}", e)))?;

        aggregation_results.push(GroupAggregationResult {
            group_value,
            result,
        });
    }

    Ok(Json(AggregationResponse {
        total_groups: aggregation_results.len(),
        results: aggregation_results,
        aggregation_function: params.agg_fn,
        group_by_path: params.group_by,
    }))
}

fn validate_jsonb_path(path: &str) -> Result<(), AppError> {
    // Basic validation - must start with $. and contain only allowed characters
    if !path.starts_with("$.") {
        return Err(AppError::Validation(
            "JSONB path must start with '$.'".to_string()
        ));
    }

    // Check against allowlist for security
    if !ALLOWED_JSONB_PATHS.contains(&path) {
        return Err(AppError::Validation(
            format!("JSONB path '{}' is not allowed. Allowed paths: {:?}", path, ALLOWED_JSONB_PATHS)
        ));
    }

    // Additional validation - no SQL injection patterns
    let forbidden_patterns = &["--", "/*", "*/", ";", "DROP", "DELETE", "UPDATE", "INSERT"];
    let upper_path = path.to_uppercase();
    for pattern in forbidden_patterns {
        if upper_path.contains(pattern) {
            return Err(AppError::Validation(
                format!("JSONB path contains forbidden pattern: {}", pattern)
            ));
        }
    }

    Ok(())
}

#[derive(Debug)]
enum BindValue {
    String(String),
    I64(i64),
}

fn build_aggregation_query(params: &AggregationParams) -> Result<(String, Vec<BindValue>), AppError> {
    let mut query = String::from("SELECT ");
    let mut bind_values = Vec::new();
    let mut bind_index = 1;

    // Add the grouping column
    query.push_str(&format!("event_data->'{}' as group_value, ",
        params.group_by.trim_start_matches("$.")));

    // Add the aggregation function
    query.push_str(&params.agg_fn.sql_expr(params.value_path.as_deref())?);
    query.push_str(" as agg_result");

    query.push_str(" FROM events WHERE 1=1");

    // Add filters
    if let Some(ref contract_id) = params.contract_id {
        query.push_str(&format!(" AND contract_id = ${}", bind_index));
        bind_values.push(BindValue::String(contract_id.clone()));
        bind_index += 1;
    }

    if let Some(ref contract_ids) = params.contract_ids {
        let ids: Vec<&str> = contract_ids.split(',').map(|s| s.trim()).collect();
        if ids.len() > 20 {
            return Err(AppError::Validation("Maximum 20 contract IDs allowed".to_string()));
        }

        let placeholders: Vec<String> = (0..ids.len())
            .map(|i| format!("${}", bind_index + i))
            .collect();
        query.push_str(&format!(" AND contract_id IN ({})", placeholders.join(", ")));

        for id in ids {
            bind_values.push(BindValue::String(id.to_string()));
            bind_index += 1;
        }
    }

    if let Some(from_ledger) = params.from_ledger {
        query.push_str(&format!(" AND ledger >= ${}", bind_index));
        bind_values.push(BindValue::I64(from_ledger));
        bind_index += 1;
    }

    if let Some(to_ledger) = params.to_ledger {
        query.push_str(&format!(" AND ledger <= ${}", bind_index));
        bind_values.push(BindValue::I64(to_ledger));
    }

    // Add GROUP BY and ORDER BY
    query.push_str(&format!(" GROUP BY event_data->'{}' ORDER BY agg_result DESC",
        params.group_by.trim_start_matches("$.")));

    // Add LIMIT for complexity control
    let limit = params.limit.unwrap_or(100).min(MAX_AGGREGATION_GROUPS);
    query.push_str(&format!(" LIMIT {}", limit));

    Ok((query, bind_values))
}

// ──────────────────────────────────────────────────────────────────────────────
// Windowed aggregation rules
// ──────────────────────────────────────────────────────────────────────────────

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

/// Result of evaluating one aggregation window.
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
        .and_then(|gb| serde_json::to_value(&gb).ok());

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
    let aggregated_data = aggregate_window(&rule, events);

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

/// Apply a rule's filter and field selectors to the events of one window,
/// returning a JSON object keyed by selector alias (or path).
fn aggregate_window(rule: &AggregationRule, events: Vec<(Uuid, Value)>) -> Value {
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
    aggregated_data
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
    let summary = NumericSummary::from_values(events.iter().filter_map(|(_, event)| {
        event.pointer(&selector.path).and_then(|v| v.as_f64())
    }));

    match selector.operation {
        AggregationOp::Count => json!(events.len()),
        AggregationOp::Sum => json!(summary.sum),
        AggregationOp::Avg => json!(summary.avg()),
        AggregationOp::Min => json!(summary.min),
        AggregationOp::Max => json!(summary.max),
        AggregationOp::DistinctCount => {
            let seen: HashSet<String> = events
                .iter()
                .filter_map(|(_, event)| event.pointer(&selector.path))
                .map(|v| v.to_string())
                .collect();
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
// Issue #934: group metrics, group configuration, aggregation optimizer
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
        let batch_size = batch_size.clamp(1, 1000);
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
    let mut group_distinct: HashMap<String, HashSet<String>> = HashMap::new();

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

        // Collect the numeric value at the same path for stat operations;
        // the group exists even if the value is non-numeric.
        let values = groups.entry(key_value.clone()).or_default();
        if let Some(num) = event.pointer(&pointer).and_then(|v| v.as_f64()) {
            values.push(num);
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

            let summary = NumericSummary::from_values(values);
            let sum_value = (summary.count > 0).then_some(summary.sum);

            GroupMetrics {
                group_key,
                rule_id,
                subscription_id,
                window_start,
                window_end,
                event_count,
                avg_value: summary.avg(),
                min_value: summary.min,
                max_value: summary.max,
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
        .map(|row| GroupMetrics {
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
        })
        .collect();

    Ok(metrics)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query_params(agg_fn: AggregationOp, value_path: Option<&str>) -> AggregationParams {
        AggregationParams {
            group_by: "$.token".to_string(),
            agg_fn,
            value_path: value_path.map(str::to_string),
            contract_id: None,
            contract_ids: None,
            from_ledger: None,
            to_ledger: None,
            limit: None,
        }
    }

    // ── shared building blocks ────────────────────────────────────────────────

    #[test]
    fn aggregation_op_wire_names_are_stable() {
        for (op, name) in [
            (AggregationOp::Count, "count"),
            (AggregationOp::Sum, "sum"),
            (AggregationOp::Avg, "avg"),
            (AggregationOp::Min, "min"),
            (AggregationOp::Max, "max"),
            (AggregationOp::DistinctCount, "distinct_count"),
        ] {
            assert_eq!(op.as_str(), name);
            assert_eq!(serde_json::to_value(op).unwrap(), json!(name));
            assert_eq!(serde_json::from_value::<AggregationOp>(json!(name)).unwrap(), op);
        }
    }

    #[test]
    fn numeric_summary_empty_and_non_empty() {
        let empty = NumericSummary::from_values(std::iter::empty());
        assert_eq!(empty.count, 0);
        assert_eq!(empty.sum, 0.0);
        assert_eq!((empty.min, empty.max, empty.avg()), (None, None, None));

        let s = NumericSummary::from_values([4.0, -2.0, 10.0]);
        assert_eq!(s.count, 3);
        assert_eq!(s.sum, 12.0);
        assert_eq!((s.min, s.max, s.avg()), (Some(-2.0), Some(10.0), Some(4.0)));
    }

    // ── on-demand queries ─────────────────────────────────────────────────────

    #[test]
    fn test_validate_jsonb_path() {
        // Valid paths
        assert!(validate_jsonb_path("$.token").is_ok());
        assert!(validate_jsonb_path("$.amount").is_ok());

        // Invalid paths
        assert!(validate_jsonb_path("token").is_err()); // Missing $.
        assert!(validate_jsonb_path("$.invalid").is_err()); // Not in allowlist
        assert!(validate_jsonb_path("$.token; DROP TABLE").is_err()); // SQL injection
    }

    #[test]
    fn test_build_aggregation_query() {
        let params = AggregationParams {
            group_by: "$.token".to_string(),
            agg_fn: AggregationFunction::Count,
            value_path: None,
            contract_id: Some("CABC123".to_string()),
            contract_ids: None,
            from_ledger: Some(1000),
            to_ledger: Some(2000),
            limit: Some(50),
        };

        let (query, bind_values) = build_aggregation_query(&params).unwrap();

        assert!(query.contains("(COUNT(*))::float8 as agg_result"));
        assert!(query.contains("GROUP BY event_data->'token'"));
        assert!(query.contains("contract_id = $1"));
        assert!(query.contains("ledger >= $2"));
        assert!(query.contains("ledger <= $3"));
        assert!(query.contains("LIMIT 50"));

        assert_eq!(bind_values.len(), 3);
    }

    #[test]
    fn value_ops_require_value_path() {
        for op in [
            AggregationOp::Sum,
            AggregationOp::Avg,
            AggregationOp::Min,
            AggregationOp::Max,
            AggregationOp::DistinctCount,
        ] {
            let err = build_aggregation_query(&query_params(op, None)).unwrap_err();
            assert!(
                matches!(err, AppError::Validation(ref m) if m.contains(op.as_str())),
                "{op:?}: {err:?}"
            );
        }
    }

    // ── windowed rules ────────────────────────────────────────────────────────

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
        // The group key is non-numeric, so there are no numeric stats.
        assert_eq!(ca1.sum_value, None);
        assert_eq!(ca1.distinct_count, Some(1));

        let ca2 = metrics.iter().find(|m| m.group_key == "\"CA2\"").unwrap();
        assert_eq!(ca2.event_count, 1);
    }

    #[test]
    fn test_compute_group_metrics_numeric_key() {
        let now = Utc::now();
        let config = GroupConfiguration {
            group_key: "/ledger".to_string(),
            aggregation_ops: vec![AggregationOp::Sum],
            window_type: WindowType::Tumbling,
        };
        let events: Vec<(Uuid, Value)> = vec![
            (Uuid::new_v4(), json!({ "ledger": 7 })),
            (Uuid::new_v4(), json!({ "ledger": 7 })),
        ];
        let metrics =
            compute_group_metrics(Uuid::new_v4(), Uuid::new_v4(), now, now, &events, &config);
        assert_eq!(metrics.len(), 1);
        let m = &metrics[0];
        assert_eq!(m.event_count, 2);
        assert_eq!(m.sum_value, Some(14.0));
        assert_eq!((m.min_value, m.max_value, m.avg_value), (Some(7.0), Some(7.0), Some(7.0)));
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

    // ── end-to-end through the merged module ──────────────────────────────────

    /// Drives both entry points with every operation: the on-demand path from
    /// request parameters to SQL, and the windowed path from a stored rule
    /// through filtering, field selection and per-group metrics. Both paths
    /// share `AggregationOp` and `NumericSummary`, so their results must agree.
    #[test]
    fn merged_aggregation_end_to_end() {
        let ops = [
            AggregationOp::Count,
            AggregationOp::Sum,
            AggregationOp::Avg,
            AggregationOp::Min,
            AggregationOp::Max,
            AggregationOp::DistinctCount,
        ];

        // On-demand path: the request deserializes, validates and builds SQL
        // whose aggregate always decodes as f64.
        for op in ops {
            let raw = json!({ "group_by": "$.token", "agg_fn": op.as_str(), "value_path": "$.amount" });
            let params: AggregationParams = serde_json::from_value(raw).unwrap();
            validate_jsonb_path(&params.group_by).unwrap();
            validate_jsonb_path(params.value_path.as_deref().unwrap()).unwrap();
            let (sql, binds) = build_aggregation_query(&params).unwrap();
            assert!(sql.contains("::float8 as agg_result"), "{op:?}: {sql}");
            assert!(sql.contains("LIMIT 100"), "{op:?}: {sql}");
            assert!(binds.is_empty());
            if op != AggregationOp::Count {
                assert!(sql.contains("(event_data->>'amount')"), "{op:?}: {sql}");
            }
        }

        // Windowed path: a rule with one selector per operation and a filter.
        let selectors: Vec<FieldSelector> = ops
            .iter()
            .map(|&op| FieldSelector {
                path: "/amount".to_string(),
                operation: op,
                alias: Some(op.as_str().to_string()),
            })
            .collect();
        let now = Utc::now();
        let rule = AggregationRule {
            id: Uuid::new_v4(),
            subscription_id: Uuid::new_v4(),
            name: "transfers".to_string(),
            description: None,
            window_type: "tumbling".to_string(),
            window_size_secs: 60,
            slide_interval_secs: None,
            fields: serde_json::to_value(&selectors).unwrap(),
            group_by: None,
            filter_condition: Some("/transfer".to_string()),
            enabled: true,
            created_at: now,
            updated_at: now,
        };
        let events: Vec<(Uuid, Value)> = vec![
            (Uuid::new_v4(), json!({ "transfer": true, "amount": 10.0 })),
            (Uuid::new_v4(), json!({ "transfer": true, "amount": 30.0 })),
            (Uuid::new_v4(), json!({ "transfer": true, "amount": 30.0 })),
            (Uuid::new_v4(), json!({ "transfer": false, "amount": 1000.0 })),
        ];

        let data = aggregate_window(&rule, events.clone());
        assert_eq!(data["count"], json!(3));
        assert_eq!(data["sum"], json!(70.0));
        assert!((data["avg"].as_f64().unwrap() - 70.0 / 3.0).abs() < 1e-9);
        assert_eq!(data["min"], json!(10.0));
        assert_eq!(data["max"], json!(30.0));
        assert_eq!(data["distinct_count"], json!(2));

        // Group metrics over the same filtered events agree with the selectors.
        let filtered: Vec<_> = events
            .into_iter()
            .filter(|(_, e)| apply_filter(e, "/transfer"))
            .collect();
        let config = GroupConfiguration {
            group_key: "amount".to_string(),
            aggregation_ops: ops.to_vec(),
            window_type: WindowType::Tumbling,
        };
        let mut metrics =
            compute_group_metrics(rule.id, rule.subscription_id, now, now, &filtered, &config);
        metrics.sort_by(|a, b| a.min_value.partial_cmp(&b.min_value).unwrap());
        assert_eq!(metrics.len(), 2);
        assert_eq!(metrics.iter().map(|m| m.event_count).sum::<i64>(), data["count"].as_i64().unwrap());
        assert_eq!(
            metrics.iter().filter_map(|m| m.sum_value).sum::<f64>(),
            data["sum"].as_f64().unwrap()
        );
        assert_eq!(metrics[0].min_value, data["min"].as_f64());
        assert_eq!(metrics[1].max_value, data["max"].as_f64());

        // Empty windows produce nulls rather than ±infinity.
        let empty = aggregate_window(&rule, Vec::new());
        assert_eq!(empty["count"], json!(0));
        assert_eq!(empty["sum"], json!(0.0));
        assert_eq!(empty["avg"], Value::Null);
        assert_eq!(empty["min"], Value::Null);
        assert_eq!(empty["max"], Value::Null);
    }
}
