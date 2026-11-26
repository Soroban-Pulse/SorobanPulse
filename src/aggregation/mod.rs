//! Event aggregation (Issue #1243).
//!
//! This module merges the former `aggregation.rs` and `event_aggregation.rs`,
//! which each defined their own aggregation operator enum and their own
//! sum/avg/min/max arithmetic. Both now share [`AggregationOp`] and
//! [`NumericSummary`].
//!
//! | Submodule    | Former file            | Purpose                                                    |
//! |--------------|------------------------|------------------------------------------------------------|
//! | [`query`]    | `aggregation.rs`       | Ad-hoc SQL `GROUP BY` aggregation over `event_data` (HTTP) |
//! | [`windowed`] | `event_aggregation.rs` | Persisted per-subscription window rules and group metrics  |
//!
//! Every public item is re-exported at `crate::aggregation::*`.
//!
//! Renamed during the merge:
//! - `aggregation::AggregationFunction` → [`AggregationOp`] (same wire names
//!   plus `distinct_count`).
//! - `aggregation::AggregationResult` → [`QueryAggregationResult`], since
//!   `windowed` already has an `AggregationResult` (stored window results).

use serde::{Deserialize, Serialize};

pub mod query;
pub mod windowed;

pub use query::*;
pub use windowed::*;

/// Aggregation operator, shared by ad-hoc queries and windowed rules.
///
/// Serialized in lowercase snake case: `count`, `sum`, `avg`, `min`, `max`,
/// `distinct_count`.
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

impl AggregationOp {
    /// Whether the operator needs a value to aggregate (everything except `count`).
    pub fn requires_value(self) -> bool {
        !matches!(self, AggregationOp::Count)
    }

    /// SQL aggregate expression over `value_expr` (a text-valued SQL expression).
    ///
    /// The result is cast to `float8` so it always decodes as `f64`.
    pub fn sql_expr(self, value_expr: &str) -> String {
        match self {
            AggregationOp::Count => "COUNT(*)::float8".to_string(),
            AggregationOp::DistinctCount => format!("COUNT(DISTINCT {value_expr})::float8"),
            AggregationOp::Sum => format!("SUM(({value_expr})::numeric)::float8"),
            AggregationOp::Avg => format!("AVG(({value_expr})::numeric)::float8"),
            AggregationOp::Min => format!("MIN(({value_expr})::numeric)::float8"),
            AggregationOp::Max => format!("MAX(({value_expr})::numeric)::float8"),
        }
    }
}

/// Sum/min/max/count over a non-empty set of numeric values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NumericSummary {
    pub count: usize,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
}

impl NumericSummary {
    /// Summarise `values`; `None` when there are no values.
    pub fn from_values(values: &[f64]) -> Option<Self> {
        if values.is_empty() {
            return None;
        }
        Some(Self {
            count: values.len(),
            sum: values.iter().sum(),
            min: values.iter().copied().fold(f64::INFINITY, f64::min),
            max: values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        })
    }

    pub fn avg(&self) -> f64 {
        self.sum / self.count as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn op_wire_names_match_both_former_enums() {
        for (op, name) in [
            (AggregationOp::Count, "count"),
            (AggregationOp::Sum, "sum"),
            (AggregationOp::Avg, "avg"),
            (AggregationOp::Min, "min"),
            (AggregationOp::Max, "max"),
            (AggregationOp::DistinctCount, "distinct_count"),
        ] {
            assert_eq!(serde_json::to_value(op).unwrap(), json!(name));
            assert_eq!(
                serde_json::from_value::<AggregationOp>(json!(name)).unwrap(),
                op
            );
        }
    }

    #[test]
    fn numeric_summary() {
        assert_eq!(NumericSummary::from_values(&[]), None);
        let s = NumericSummary::from_values(&[4.0, -1.0, 3.0]).unwrap();
        assert_eq!((s.count, s.sum, s.min, s.max), (3, 6.0, -1.0, 4.0));
        assert!((s.avg() - 2.0).abs() < 1e-12);
    }

    /// End-to-end over the merged module: one operator list, parsed from the
    /// same JSON both callers accept, drives the SQL query builder (`query`)
    /// and the in-memory window and group evaluation (`windowed`), and they
    /// agree on what each operator means.
    #[test]
    fn merged_aggregation_path_end_to_end() {
        let ops: Vec<AggregationOp> = serde_json::from_value(json!([
            "count",
            "sum",
            "avg",
            "min",
            "max",
            "distinct_count"
        ]))
        .unwrap();

        // Ad-hoc query path: every operator builds a valid, bound query.
        for &op in &ops {
            let params = AggregationParams {
                group_by: "$.token".to_string(),
                agg_fn: op,
                value_path: Some("$.amount".to_string()),
                contract_id: Some("CABC".to_string()),
                contract_ids: None,
                from_ledger: None,
                to_ledger: None,
                limit: None,
            };
            let (sql, binds) = build_aggregation_query(&params).unwrap();
            assert!(
                sql.contains(&op.sql_expr("event_data->>'amount'")),
                "{op:?}: {sql}"
            );
            assert!(sql.contains("GROUP BY event_data->'token'"));
            assert_eq!(binds.len(), 1);
        }

        // Windowed path: same events, same operators, evaluated in memory.
        let events: Vec<(Uuid, serde_json::Value)> = vec![
            (Uuid::new_v4(), json!({ "token": "XLM", "amount": 10.0 })),
            (Uuid::new_v4(), json!({ "token": "XLM", "amount": 30.0 })),
            (Uuid::new_v4(), json!({ "token": "USDC", "amount": 30.0 })),
        ];
        let results: Vec<serde_json::Value> = ops
            .iter()
            .map(|&operation| {
                let selector = FieldSelector {
                    path: "/amount".to_string(),
                    operation,
                    alias: None,
                };
                compute_operation(&events, &selector)
            })
            .collect();
        assert_eq!(
            results,
            vec![
                json!(3),
                json!(70.0),
                json!(70.0 / 3.0),
                json!(10.0),
                json!(30.0),
                json!(2)
            ]
        );

        // Group metrics use the same NumericSummary as compute_operation.
        let now = Utc::now();
        let config = GroupConfiguration {
            group_key: "token".to_string(),
            aggregation_ops: ops.clone(),
            window_type: WindowType::Tumbling,
        };
        let by_amount: Vec<(Uuid, serde_json::Value)> = events
            .iter()
            .map(|(id, e)| (*id, json!({ "token": e["amount"] })))
            .collect();
        let metrics = compute_group_metrics(
            Uuid::new_v4(),
            Uuid::new_v4(),
            now,
            now,
            &by_amount,
            &config,
        );
        let thirty = metrics.iter().find(|m| m.group_key == "30.0").unwrap();
        assert_eq!(thirty.event_count, 2);
        assert_eq!(thirty.sum_value, Some(60.0));
        assert_eq!(
            (thirty.min_value, thirty.max_value, thirty.avg_value),
            (Some(30.0), Some(30.0), Some(30.0))
        );
    }
}
