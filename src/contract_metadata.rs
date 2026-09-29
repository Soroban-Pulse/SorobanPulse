//! Contract labels and metadata registry (#1066).
//!
//! Contract IDs (`C...` 56-char strkeys) are meaningless to humans on their
//! own. This module lets operators attach a human-readable label (name,
//! project, source repo, tags, verified flag) to a contract id so that the
//! API, dashboard and notification templates can show something friendlier
//! than a raw strkey.

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow, utoipa::ToSchema)]
pub struct ContractMetadata {
    pub contract_id: String,
    pub name: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub source_repo: Option<String>,
    pub tags: Vec<String>,
    pub verified: bool,
}

#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct UpsertContractMetadata {
    pub name: String,
    pub description: Option<String>,
    pub project_url: Option<String>,
    pub source_repo: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub verified: bool,
}

/// Fetch a single contract's metadata, if any label has been registered.
pub async fn get_contract_metadata(
    pool: &PgPool,
    contract_id: &str,
) -> sqlx::Result<Option<ContractMetadata>> {
    sqlx::query_as::<_, ContractMetadata>(
        "SELECT contract_id, name, description, project_url, source_repo, tags, verified
         FROM contract_metadata
         WHERE contract_id = $1",
    )
    .bind(contract_id)
    .fetch_optional(pool)
    .await
}

/// Batch fetch metadata for a set of contract ids, e.g. for enriching a page
/// of event responses without one query per row.
pub async fn get_contract_metadata_batch(
    pool: &PgPool,
    contract_ids: &[String],
) -> sqlx::Result<Vec<ContractMetadata>> {
    if contract_ids.is_empty() {
        return Ok(Vec::new());
    }
    sqlx::query_as::<_, ContractMetadata>(
        "SELECT contract_id, name, description, project_url, source_repo, tags, verified
         FROM contract_metadata
         WHERE contract_id = ANY($1)",
    )
    .bind(contract_ids)
    .fetch_all(pool)
    .await
}

/// Create or update a contract's label (admin only).
pub async fn upsert_contract_metadata(
    pool: &PgPool,
    contract_id: &str,
    input: UpsertContractMetadata,
) -> sqlx::Result<ContractMetadata> {
    sqlx::query_as::<_, ContractMetadata>(
        "INSERT INTO contract_metadata
            (contract_id, name, description, project_url, source_repo, tags, verified, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, NOW())
         ON CONFLICT (contract_id) DO UPDATE SET
            name = EXCLUDED.name,
            description = EXCLUDED.description,
            project_url = EXCLUDED.project_url,
            source_repo = EXCLUDED.source_repo,
            tags = EXCLUDED.tags,
            verified = EXCLUDED.verified,
            updated_at = NOW()
         RETURNING contract_id, name, description, project_url, source_repo, tags, verified",
    )
    .bind(contract_id)
    .bind(input.name)
    .bind(input.description)
    .bind(input.project_url)
    .bind(input.source_repo)
    .bind(input.tags)
    .bind(input.verified)
    .fetch_one(pool)
    .await
}

/// Remove a contract's label (admin only).
pub async fn delete_contract_metadata(pool: &PgPool, contract_id: &str) -> sqlx::Result<bool> {
    let result = sqlx::query("DELETE FROM contract_metadata WHERE contract_id = $1")
        .bind(contract_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}

/// Bulk import labels from a parsed JSON/YAML list, e.g. via the `schema_cli`
/// or a dedicated admin endpoint. Returns the number of rows written.
pub async fn bulk_import(
    pool: &PgPool,
    entries: Vec<(String, UpsertContractMetadata)>,
) -> sqlx::Result<usize> {
    let mut count = 0;
    for (contract_id, input) in entries {
        upsert_contract_metadata(pool, &contract_id, input).await?;
        count += 1;
    }
    Ok(count)
}

/// Convenience helper for enriching notification templates / event
/// responses: returns the display name if a label is known, otherwise the
/// raw contract id unchanged.
pub fn display_name(contract_id: &str, metadata: Option<&ContractMetadata>) -> String {
    match metadata {
        Some(m) => m.name.clone(),
        None => contract_id.to_string(),
    }
}
