use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct Event {
    pub id: Uuid,
    pub contract_id: String,
    pub event_type: String,
    pub tx_hash: String,
    pub ledger: i64,
    pub timestamp: DateTime<Utc>,
    pub event_data: Value,
    pub created_at: DateTime<Utc>,
    #[sqlx(default)]
    #[serde(skip)]
    pub total_count: i64,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
pub struct PaginationParams {
    pub page: Option<i64>,
    pub limit: Option<i64>,
    pub exact_count: Option<bool>,
    pub fields: Option<String>,
    pub event_type: Option<String>,
    pub from_ledger: Option<i64>,
    pub to_ledger: Option<i64>,
}

#[derive(Debug, Deserialize)]
pub struct StreamParams {
    pub contract_id: Option<String>,
}

impl PaginationParams {
    pub const ALLOWED_FIELDS: &'static [&'static str] = &[
        "id",
        "contract_id",
        "event_type",
        "tx_hash",
        "ledger",
        "timestamp",
        "event_data",
        "created_at",
    ];

    pub fn columns(&self) -> Vec<&str> {
        match &self.fields {
            Some(f) if !f.trim().is_empty() => f
                .split(',')
                .map(|s| s.trim())
                .filter(|s| Self::ALLOWED_FIELDS.contains(s))
                .collect(),
            _ => Self::ALLOWED_FIELDS.to_vec(),
        }
    }
    pub fn offset(&self) -> i64 {
        let page = self.page.unwrap_or(1).max(1);
        let limit = self.limit();
        (page - 1) * limit
    }

    pub fn limit(&self) -> i64 {
        self.limit.unwrap_or(20).clamp(1, 100)
    }
}

/// Soroban RPC response types
#[derive(Debug, Deserialize)]
pub struct RpcResponse<T> {
    pub result: Option<T>,
    pub error: Option<RpcError>,
}

#[derive(Debug, Deserialize)]
pub struct RpcError {
    #[allow(dead_code)]
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Deserialize)]
pub struct LatestLedgerResult {
    pub sequence: u64,
}

#[derive(Debug, Deserialize)]
pub struct GetEventsResult {
    pub events: Vec<SorobanEvent>,
    #[serde(rename = "latestLedger")]
    pub latest_ledger: u64,
    #[serde(rename = "cursor")]
    pub rpc_cursor: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SorobanEvent {
    #[serde(rename = "contractId")]
    pub contract_id: String,
    #[serde(rename = "type")]
    pub event_type: String,
    #[serde(rename = "txHash")]
    pub tx_hash: String,
    pub ledger: u64,
    #[serde(rename = "ledgerClosedAt")]
    pub ledger_closed_at: String,
    pub value: Value,
    pub topic: Option<Vec<Value>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(page: Option<i64>, limit: Option<i64>) -> PaginationParams {
        PaginationParams {
            page,
            limit,
            exact_count: None,
            fields: None,
            event_type: None,
            from_ledger: None,
            to_ledger: None,
        }
    }

    #[test]
    fn page_zero_offset_is_zero() {
        assert_eq!(params(Some(0), None).offset(), 0);
    }

    #[test]
    fn page_none_offset_is_zero() {
        assert_eq!(params(None, None).offset(), 0);
    }

    #[test]
    fn limit_zero_clamps_to_one() {
        assert_eq!(params(None, Some(0)).limit(), 1);
    }

    #[test]
    fn limit_over_max_clamps_to_hundred() {
        assert_eq!(params(None, Some(200)).limit(), 100);
    }

    #[test]
    fn limit_none_defaults_to_twenty() {
        assert_eq!(params(None, None).limit(), 20);
    }

    #[test]
    fn page_3_limit_10_offset_is_20() {
        assert_eq!(params(Some(3), Some(10)).offset(), 20);
    }

    // --- RPC deserialization fixture tests ---

    #[test]
    fn deserialize_get_events_success() {
        let raw = include_str!("../tests/fixtures/get_events_response.json");
        let resp: RpcResponse<GetEventsResult> = serde_json::from_str(raw).unwrap();
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result.latest_ledger, 1234600);
        assert_eq!(result.rpc_cursor.as_deref(), Some("1234567-0"));
        assert_eq!(result.events.len(), 1);
        let ev = &result.events[0];
        assert_eq!(ev.contract_id, "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM");
        assert_eq!(ev.event_type, "contract");
        assert_eq!(ev.tx_hash, "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2");
        assert_eq!(ev.ledger, 1234567);
        assert_eq!(ev.ledger_closed_at, "2026-03-14T00:00:00Z");
        assert!(ev.topic.is_some());
    }

    #[test]
    fn deserialize_get_events_error() {
        let raw = include_str!("../tests/fixtures/get_events_error.json");
        let resp: RpcResponse<GetEventsResult> = serde_json::from_str(raw).unwrap();
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, -32600);
        assert_eq!(err.message, "startLedger must be within the ledger retention window");
    }

    #[test]
    fn deserialize_get_events_empty() {
        let raw = include_str!("../tests/fixtures/get_events_empty.json");
        let resp: RpcResponse<GetEventsResult> = serde_json::from_str(raw).unwrap();
        assert!(resp.error.is_none());
        let result = resp.result.unwrap();
        assert!(result.events.is_empty());
        assert_eq!(result.latest_ledger, 1234600);
        assert!(result.rpc_cursor.is_none());
    }
}
