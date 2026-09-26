//! Per-contract resource and fee statistics (#1062).
//!
//! Disabled by default. Set `RESOURCE_STATS_ENABLED=true` to fetch `getTransaction` for
//! transactions that emitted events (one extra RPC call per new transaction).

use serde_json::{json, Value};
use sqlx::PgPool;
use std::time::Duration;

#[derive(Debug, Default, PartialEq, Clone, Copy)]
pub struct TxResources {
    pub resource_fee: i64,
    pub instructions: i64,
    pub read_bytes: i64,
    pub write_bytes: i64,
}

/// Depth-first search for the first numeric-ish value under `key`.
fn find_num(v: &Value, key: &str) -> Option<i64> {
    match v {
        Value::Object(m) => {
            if let Some(x) = m.get(key) {
                if let Some(n) = x.as_i64() {
                    return Some(n);
                }
                if let Some(n) = x.as_str().and_then(|s| s.parse().ok()) {
                    return Some(n);
                }
            }
            m.values().find_map(|c| find_num(c, key))
        }
        Value::Array(a) => a.iter().find_map(|c| find_num(c, key)),
        _ => None,
    }
}

/// Extract resource usage from a `getTransaction` result requested with `xdrFormat: "json"`.
pub fn parse_resources(result: &Value) -> TxResources {
    let env = result.get("envelopeJson").unwrap_or(&Value::Null);
    TxResources {
        resource_fee: find_num(env, "resource_fee").unwrap_or(0),
        instructions: find_num(env, "instructions").unwrap_or(0),
        read_bytes: find_num(env, "disk_read_bytes").or_else(|| find_num(env, "read_bytes")).unwrap_or(0),
        write_bytes: find_num(env, "write_bytes").unwrap_or(0),
    }
}

pub async fn fetch_transaction(client: &reqwest::Client, url: &str, tx_hash: &str) -> Result<TxResources, String> {
    let body = json!({"jsonrpc":"2.0","id":1,"method":"getTransaction",
        "params":{"hash": tx_hash, "xdrFormat": "json"}});
    let v: Value = client.post(url).json(&body).send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())?;
    v.get("result").map(parse_resources).ok_or_else(|| "getTransaction: no result".to_string())
}

/// Fetch resources for up to `batch` event transactions that have none stored.
pub async fn sync_batch(pool: &PgPool, client: &reqwest::Client, url: &str, batch: i64) -> Result<usize, String> {
    let rows: Vec<(String, String, i64)> = sqlx::query_as(
        "SELECT DISTINCT ON (e.tx_hash) e.tx_hash, e.contract_id, e.ledger
         FROM events e LEFT JOIN tx_resources r ON r.tx_hash = e.tx_hash
         WHERE r.tx_hash IS NULL ORDER BY e.tx_hash, e.ledger DESC LIMIT $1",
    )
    .bind(batch)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    let mut n = 0;
    for (hash, contract, ledger) in rows {
        let r = fetch_transaction(client, url, &hash).await?;
        sqlx::query(
            "INSERT INTO tx_resources (tx_hash, contract_id, ledger, resource_fee, instructions, read_bytes, write_bytes)
             VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (tx_hash) DO NOTHING",
        )
        .bind(&hash).bind(&contract).bind(ledger)
        .bind(r.resource_fee).bind(r.instructions).bind(r.read_bytes).bind(r.write_bytes)
        .execute(pool).await.map_err(|e| e.to_string())?;
        n += 1;
    }
    if n > 0 {
        let _ = sqlx::query("REFRESH MATERIALIZED VIEW CONCURRENTLY mv_contract_resources_daily").execute(pool).await;
    }
    Ok(n)
}

pub fn enabled() -> bool {
    std::env::var("RESOURCE_STATS_ENABLED").map(|v| v == "true" || v == "1").unwrap_or(false)
}

pub fn spawn_sync(pool: PgPool, url: String) {
    if !enabled() {
        return;
    }
    tokio::spawn(async move {
        let client = reqwest::Client::new();
        let mut tick = tokio::time::interval(Duration::from_secs(30));
        loop {
            tick.tick().await;
            if let Err(e) = sync_batch(&pool, &client, &url, 50).await {
                tracing::warn!(error = %e, "resource stats sync failed");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_nested_resources() {
        let v = json!({"envelopeJson":{"tx":{"tx":{"ext":{"v1":{"resource_fee":"1200",
            "resources":{"instructions":5000,"read_bytes":10,"write_bytes":20}}}}}}});
        assert_eq!(parse_resources(&v), TxResources{resource_fee:1200,instructions:5000,read_bytes:10,write_bytes:20});
    }
}
