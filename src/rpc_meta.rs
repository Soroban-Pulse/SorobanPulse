//! RPC retention-gap detection (#1060) and protocol version negotiation (#1061).

use serde_json::{json, Value};
use sqlx::PgPool;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Duration;

use crate::config::IndexerState;

/// Supported Stellar protocol range (inclusive).
pub const SUPPORTED_PROTOCOL_MIN: u32 = 21;
pub const SUPPORTED_PROTOCOL_MAX: u32 = 25;

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct RpcVersionInfo {
    pub version: String,
    pub commit_hash: String,
    pub protocol_version: u32,
    pub supported: bool,
}

static VERSION: OnceLock<RwLock<Option<RpcVersionInfo>>> = OnceLock::new();

fn slot() -> &'static RwLock<Option<RpcVersionInfo>> {
    VERSION.get_or_init(|| RwLock::new(None))
}

/// Latest RPC version info, if fetched.
pub fn current_version() -> Option<RpcVersionInfo> {
    slot().read().ok().and_then(|g| g.clone())
}

pub fn protocol_supported(p: u32) -> bool {
    (SUPPORTED_PROTOCOL_MIN..=SUPPORTED_PROTOCOL_MAX).contains(&p)
}

async fn rpc_call(client: &reqwest::Client, url: &str, method: &str) -> Result<Value, String> {
    let body = json!({"jsonrpc": "2.0", "id": 1, "method": method});
    let v: Value = client
        .post(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .json()
        .await
        .map_err(|e| e.to_string())?;
    v.get("result").cloned().ok_or_else(|| format!("{method}: no result in response"))
}

pub fn parse_version_info(result: &Value) -> RpcVersionInfo {
    let protocol_version = result
        .get("protocolVersion")
        .or_else(|| result.get("protocol_version"))
        .and_then(Value::as_u64)
        .unwrap_or(0) as u32;
    RpcVersionInfo {
        version: result.get("version").and_then(Value::as_str).unwrap_or("").to_string(),
        commit_hash: result
            .get("commitHash")
            .or_else(|| result.get("commit_hash"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        protocol_version,
        supported: protocol_supported(protocol_version),
    }
}

/// Fetch `getVersionInfo`, log it, warn (or fail when `strict`) outside the supported range.
pub async fn negotiate_version(client: &reqwest::Client, url: &str, strict: bool) -> Result<RpcVersionInfo, String> {
    let info = parse_version_info(&rpc_call(client, url, "getVersionInfo").await?);
    tracing::info!(
        protocol_version = info.protocol_version,
        version = %info.version,
        commit_hash = %info.commit_hash,
        "RPC version info"
    );
    metrics::gauge!(
        "soroban_pulse_rpc_info",
        "version" => info.version.clone(),
        "commit_hash" => info.commit_hash.clone(),
        "protocol_version" => info.protocol_version.to_string()
    )
    .set(1.0);
    if !info.supported {
        let msg = format!(
            "RPC protocol {} is outside supported range {}-{}; upgrade SorobanPulse or point STELLAR_RPC_URL at a compatible RPC (see docs/runbooks/rpc-errors.md)",
            info.protocol_version, SUPPORTED_PROTOCOL_MIN, SUPPORTED_PROTOCOL_MAX
        );
        if strict {
            return Err(msg);
        }
        tracing::warn!("{msg}");
    }
    if let Ok(mut g) = slot().write() {
        *g = Some(info.clone());
    }
    Ok(info)
}

/// Decide whether a checkpoint lies below the RPC's oldest retained ledger.
/// Returns the missing inclusive range.
pub fn detect_gap(checkpoint: u64, oldest_ledger: u64) -> Option<(u64, u64)> {
    (checkpoint > 0 && oldest_ledger > 0 && checkpoint < oldest_ledger).then(|| (checkpoint, oldest_ledger - 1))
}

pub async fn record_gap(pool: &PgPool, from: u64, to: u64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO indexer_gaps (from_ledger, to_ledger) VALUES ($1, $2) ON CONFLICT DO NOTHING")
        .bind(from as i64)
        .bind(to as i64)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn list_gaps(pool: &PgPool) -> Vec<Value> {
    let rows: Vec<(i64, i64, chrono::DateTime<chrono::Utc>)> =
        sqlx::query_as("SELECT from_ledger, to_ledger, detected_at FROM indexer_gaps ORDER BY from_ledger")
            .fetch_all(pool)
            .await
            .unwrap_or_default();
    rows.into_iter()
        .map(|(f, t, d)| json!({"from_ledger": f, "to_ledger": t, "ledgers": t - f + 1, "detected_at": d}))
        .collect()
}

/// Check retention once; records gap in logs, metrics and DB.
pub async fn check_gap(client: &reqwest::Client, url: &str, pool: &PgPool, checkpoint: u64) -> Result<Option<(u64, u64)>, String> {
    let health = rpc_call(client, url, "getHealth").await?;
    let oldest = health.get("oldestLedger").and_then(Value::as_u64).unwrap_or(0);
    let gap = detect_gap(checkpoint, oldest);
    match gap {
        Some((from, to)) => {
            tracing::error!(
                from_ledger = from,
                to_ledger = to,
                oldest_ledger = oldest,
                "RPC retention gap: ledgers {from}-{to} are no longer available from RPC; run the backfill job (see docs/backfill.md)"
            );
            metrics::gauge!("soroban_pulse_indexer_gap_ledgers").set((to - from + 1) as f64);
            record_gap(pool, from, to).await.map_err(|e| e.to_string())?;
        }
        None => metrics::gauge!("soroban_pulse_indexer_gap_ledgers").set(0.0),
    }
    Ok(gap)
}

/// Background monitor: version negotiation at start, gap check every `every` interval.
pub fn spawn_monitor(pool: PgPool, url: String, state: Option<Arc<IndexerState>>, start_ledger: u64) {
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let strict = std::env::var("RPC_PROTOCOL_STRICT").map(|v| v == "true" || v == "1").unwrap_or(false);
        let mut tick = tokio::time::interval(Duration::from_secs(60));
        loop {
            tick.tick().await;
            if let Err(e) = negotiate_version(&client, &url, strict).await {
                tracing::error!(error = %e, "RPC version negotiation failed");
            }
            let cp = state
                .as_ref()
                .map(|s| s.current_ledger.load(std::sync::atomic::Ordering::Relaxed))
                .filter(|c| *c > 0)
                .unwrap_or(start_ledger);
            if let Err(e) = check_gap(&client, &url, &pool, cp).await {
                tracing::warn!(error = %e, "RPC retention gap check failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gap_detected_below_oldest() {
        assert_eq!(detect_gap(100, 500), Some((100, 499)));
        assert_eq!(detect_gap(500, 500), None);
        assert_eq!(detect_gap(0, 500), None);
    }

    #[test]
    fn parses_version_info() {
        let i = parse_version_info(&json!({"version":"23.0.0","commitHash":"abc","protocolVersion":23}));
        assert_eq!(i.protocol_version, 23);
        assert!(i.supported);
        assert!(!parse_version_info(&json!({"protocolVersion": 99})).supported);
    }
}
