//! Track contract deployments and WASM upgrades (issue #1058).

use axum::extract::{Path, State};
use axum::Json;
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::time::Duration;

use crate::error::AppError;
use crate::routes::AppState;

/// Compare `current_hash` with the latest stored version. Inserts a row when the
/// contract is new or the hash changed. Returns `Some(is_upgrade)` if a row was added.
pub async fn record_hash(pool: &PgPool, contract_id: &str, current_hash: &str) -> Result<Option<bool>, sqlx::Error> {
    let last: Option<String> = sqlx::query_scalar(
        "SELECT wasm_hash FROM contract_versions WHERE contract_id = $1 ORDER BY id DESC LIMIT 1",
    ).bind(contract_id).fetch_optional(pool).await?;
    if last.as_deref() == Some(current_hash) {
        return Ok(None);
    }
    let upgrade = last.is_some();
    sqlx::query(
        "INSERT INTO contract_versions (contract_id, wasm_hash, first_seen_ledger, is_upgrade)
         VALUES ($1, $2, (SELECT MAX(ledger) FROM events WHERE contract_id = $1), $3)",
    ).bind(contract_id).bind(current_hash).bind(upgrade).execute(pool).await?;
    Ok(Some(upgrade))
}

/// One poll cycle over recently active contracts. On an upgrade it emits an
/// internal `contract_upgraded` event and re-triggers spec fetching.
pub async fn run_once(
    pool: &PgPool,
    client: &reqwest::Client,
    rpc_url: &str,
    event_tx: &tokio::sync::broadcast::Sender<crate::models::SorobanEvent>,
    batch: i64,
) {
    let ids: Vec<String> = sqlx::query_scalar(
        "SELECT contract_id FROM events WHERE created_at > NOW() - INTERVAL '1 day'
         GROUP BY contract_id ORDER BY MAX(created_at) DESC LIMIT $1",
    ).bind(batch).fetch_all(pool).await.unwrap_or_default();
    for id in ids {
        tokio::time::sleep(Duration::from_millis(250)).await;
        let Some(hash) = crate::contract_specs::fetch_wasm_hash(client, rpc_url, &id).await else { continue };
        match record_hash(pool, &id, &hash).await {
            Ok(Some(true)) => {
                tracing::info!(contract_id = %id, wasm_hash = %hash, "contract upgraded");
                let _ = event_tx.send(crate::models::SorobanEvent {
                    id: None,
                    contract_id: id.clone(),
                    event_type: "contract_upgraded".into(),
                    tx_hash: String::new(),
                    ledger: 0,
                    ledger_closed_at: chrono::Utc::now().to_rfc3339(),
                    ledger_hash: None,
                    in_successful_call: true,
                    ..Default::default()
                });
                if crate::contract_specs::enabled() {
                    crate::contract_specs::store_spec_for(pool, client, rpc_url, &id, &hash).await;
                }
            }
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %e, "contract version record failed"),
        }
    }
}

/// Background worker; runs every 30 seconds.
pub async fn run_worker(pool: PgPool, rpc_url: String, event_tx: tokio::sync::broadcast::Sender<crate::models::SorobanEvent>) {
    let client = reqwest::Client::new();
    loop {
        run_once(&pool, &client, &rpc_url, &event_tx, 50).await;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

/// `GET /v1/contracts/{id}/versions`
pub async fn get_contract_versions(
    State(state): State<AppState>,
    Path(contract_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    crate::handlers::validate_contract_id(&contract_id)?;
    let rows = sqlx::query(
        "SELECT wasm_hash, first_seen_ledger, observed_at, is_upgrade FROM contract_versions
         WHERE contract_id = $1 ORDER BY id DESC",
    ).bind(&contract_id).fetch_all(&state.read_pool).await?;
    let mut versions = Vec::new();
    for r in rows {
        versions.push(json!({
            "wasm_hash": r.try_get::<String,_>("wasm_hash")?,
            "first_seen_ledger": r.try_get::<Option<i64>,_>("first_seen_ledger")?,
            "observed_at": r.try_get::<chrono::DateTime<chrono::Utc>,_>("observed_at")?,
            "is_upgrade": r.try_get::<bool,_>("is_upgrade")?,
        }));
    }
    Ok(Json(json!({ "contract_id": contract_id, "versions": versions })))
}
