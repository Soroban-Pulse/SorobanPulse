//! Issue #617: JSON Schema validation for Soroban event data.
//!
//! Compiles and caches JSON Draft-7 schemas per contract. Schemas are stored in the
//! `contract_schemas` table and loaded at startup. Each schema update increments a
//! `version` counter (via DB trigger) so consumers can detect stale cached copies.
//!
//! ## Integration points
//! - `register_schema` / `get_schema` / `delete_schema` / `list_schemas` — CRUD.
//! - `validate_event_data` — called by the indexer to gate event storage.
//! - `record_validation_metrics` — persists pass/fail counts to `schema_validation_metrics`.

use jsonschema::{Draft, JSONSchema};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn};

use crate::error::ValidationErrorDetail;
use crate::metrics;

/// Summary returned when listing all registered schemas.
#[derive(Debug, Serialize)]
pub struct SchemaInfo {
    pub contract_id: String,
    pub version: i32,
    pub description: Option<String>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Schema validator that caches compiled JSON schemas per contract.
#[derive(Clone)]
pub struct SchemaValidator {
    pool: PgPool,
    cache: Arc<RwLock<HashMap<String, Arc<JSONSchema>>>>,
}

impl SchemaValidator {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load all schemas from the database into the in-memory cache.
    pub async fn load_schemas(&self) -> Result<(), sqlx::Error> {
        let schemas: Vec<(String, Value)> =
            sqlx::query_as("SELECT contract_id, schema FROM contract_schemas")
                .fetch_all(&self.pool)
                .await?;

        let mut cache = self.cache.write().await;
        for (contract_id, schema_value) in schemas {
            match JSONSchema::options()
                .with_draft(Draft::Draft7)
                .compile(&schema_value)
            {
                Ok(compiled) => {
                    cache.insert(contract_id.clone(), Arc::new(compiled));
                    debug!(contract_id = %contract_id, "Loaded schema for contract");
                }
                Err(e) => {
                    warn!(contract_id = %contract_id, error = %e, "Failed to compile schema");
                }
            }
        }
        Ok(())
    }

    /// Register (or replace) a JSON Schema for a contract.
    ///
    /// The `version` column is incremented automatically by the DB trigger on UPDATE.
    pub async fn register_schema(
        &self,
        contract_id: &str,
        schema: &Value,
    ) -> Result<(), anyhow::Error> {
        register_schema_with_desc(self, contract_id, schema, None).await
    }

    /// Register a schema with an optional human-readable description.
    pub async fn register_schema_described(
        &self,
        contract_id: &str,
        schema: &Value,
        description: Option<&str>,
    ) -> Result<(), anyhow::Error> {
        register_schema_with_desc(self, contract_id, schema, description).await
    }

    /// Validate event data against the registered schema for a contract.
    ///
    /// Returns:
    /// - `None` — no schema is registered for this contract (event is accepted).
    /// - `Some((true, []))` — validation passed.
    /// - `Some((false, errors))` — validation failed with structured error details.
    ///
    /// Increments Prometheus counters (`schema_validation_pass/fail_total`) on every call.
    pub async fn validate_event_data(
        &self,
        contract_id: &str,
        event_data: &Value,
    ) -> Option<(bool, Vec<ValidationErrorDetail>)> {
        let cache = self.cache.read().await;
        let schema = cache.get(contract_id)?;

        let is_valid = schema.is_valid(event_data);

        if !is_valid {
            if let Err(errors) = schema.validate(event_data) {
                let error_details: Vec<ValidationErrorDetail> = errors
                    .map(|e| ValidationErrorDetail {
                        instance_path: e.instance_path.to_string(),
                        schema_path: e.schema_path.to_string(),
                        message: e.to_string(),
                    })
                    .collect();

                let error_messages: Vec<String> = error_details
                    .iter()
                    .map(|e| format!("{} at {}", e.message, e.instance_path))
                    .collect();

                warn!(
                    contract_id = %contract_id,
                    errors = ?error_messages,
                    "Event data failed schema validation"
                );

                metrics::record_schema_validation_fail(contract_id);
                return Some((false, error_details));
            }
        }

        metrics::record_schema_validation_pass(contract_id);
        Some((is_valid, vec![]))
    }

    /// Retrieve the full schema JSON and its current version for a contract.
    pub async fn get_schema(&self, contract_id: &str) -> Option<Value> {
        sqlx::query_scalar::<_, Value>(
            "SELECT schema FROM contract_schemas WHERE contract_id = $1",
        )
        .bind(contract_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    /// Retrieve schema metadata (without the full schema body) for a contract.
    pub async fn get_schema_info(&self, contract_id: &str) -> Option<SchemaInfo> {
        sqlx::query_as::<_, (String, i32, Option<String>, chrono::DateTime<chrono::Utc>)>(
            "SELECT contract_id, version, description, updated_at \
             FROM contract_schemas WHERE contract_id = $1",
        )
        .bind(contract_id)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
        .map(|(cid, version, desc, updated_at)| SchemaInfo {
            contract_id: cid,
            version,
            description: desc,
            updated_at,
        })
    }

    /// List all registered schemas (metadata only, no schema body).
    pub async fn list_schemas(&self) -> Result<Vec<SchemaInfo>, sqlx::Error> {
        let rows = sqlx::query_as::<_, (String, i32, Option<String>, chrono::DateTime<chrono::Utc>)>(
            "SELECT contract_id, version, description, updated_at \
             FROM contract_schemas ORDER BY contract_id",
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|(contract_id, version, description, updated_at)| SchemaInfo {
                contract_id,
                version,
                description,
                updated_at,
            })
            .collect())
    }

    /// Delete the schema for a contract and evict it from the cache.
    pub async fn delete_schema(&self, contract_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM contract_schemas WHERE contract_id = $1")
            .bind(contract_id)
            .execute(&self.pool)
            .await?;

        if result.rows_affected() > 0 {
            let mut cache = self.cache.write().await;
            cache.remove(contract_id);
            debug!(contract_id = %contract_id, "Deleted schema for contract");
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Persist cumulative pass/fail counters to `schema_validation_metrics`.
    ///
    /// Called periodically to keep the DB table in sync with the in-process counters.
    /// Uses `ON CONFLICT DO UPDATE` so the row is upserted atomically.
    pub async fn record_validation_metrics(
        pool: &PgPool,
        contract_id: &str,
        pass_delta: i64,
        fail_delta: i64,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO schema_validation_metrics (contract_id, pass_count, fail_count, last_checked)
            VALUES ($1, $2, $3, NOW())
            ON CONFLICT (contract_id) DO UPDATE
                SET pass_count   = schema_validation_metrics.pass_count + EXCLUDED.pass_count,
                    fail_count   = schema_validation_metrics.fail_count + EXCLUDED.fail_count,
                    last_checked = NOW()
            "#,
        )
        .bind(contract_id)
        .bind(pass_delta)
        .bind(fail_delta)
        .execute(pool)
        .await?;
        Ok(())
    }
}

/// Internal helper shared by `register_schema` and `register_schema_described`.
async fn register_schema_with_desc(
    sv: &SchemaValidator,
    contract_id: &str,
    schema: &Value,
    description: Option<&str>,
) -> Result<(), anyhow::Error> {
    let compiled = JSONSchema::options()
        .with_draft(Draft::Draft7)
        .compile(schema)
        .map_err(|e| anyhow::anyhow!("Invalid JSON Schema: {}", e))?;

    sqlx::query(
        r#"
        INSERT INTO contract_schemas (contract_id, schema, description, updated_at)
        VALUES ($1, $2, $3, NOW())
        ON CONFLICT (contract_id)
        DO UPDATE SET schema = EXCLUDED.schema,
                      description = COALESCE(EXCLUDED.description, contract_schemas.description),
                      updated_at = NOW()
        "#,
    )
    .bind(contract_id)
    .bind(schema)
    .bind(description)
    .execute(&sv.pool)
    .await?;

    let mut cache = sv.cache.write().await;
    cache.insert(contract_id.to_string(), Arc::new(compiled));

    debug!(contract_id = %contract_id, "Registered schema for contract");
    Ok(())
}

// === Schema registry and evolution (Issue #816)

use serde::Deserialize;
use std::collections::HashSet;

/// How strictly an event is validated against its schema.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ValidationMode {
    /// Any schema violation rejects the event.
    #[serde(rename = "strict")]
    Strict,
    /// Violations are recorded and reported but the event is accepted.
    #[serde(rename = "lenient")]
    Lenient,
}

impl Default for ValidationMode {
    fn default() -> Self {
        ValidationMode::Strict
    }
}

/// Outcome of a mode-aware validation call.
#[derive(Debug, Serialize)]
pub struct ValidationOutcome {
    pub valid: bool,
    /// False only when strict mode rejects the event.
    pub accepted: bool,
    pub mode: ValidationMode,
    pub errors: Vec<ValidationErrorDetail>,
}

/// Compatibility guarantee required of a new schema version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CompatibilityMode {
    /// New schema can read data written with the old schema.
    #[serde(rename = "backward")]
    Backward,
    /// Old schema can read data written with the new schema.
    #[serde(rename = "forward")]
    Forward,
    /// Both directions hold.
    #[serde(rename = "full")]
    Full,
    /// No checking.
    #[serde(rename = "none")]
    None,
}

/// A single difference between two schema versions.
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub enum SchemaChange {
    FieldAdded { field: String, required: bool },
    FieldRemoved { field: String, required: bool },
    FieldTypeChanged { field: String, from: String, to: String },
    FieldBecameRequired { field: String },
    FieldBecameOptional { field: String },
}

impl SchemaChange {
    /// A change is breaking when a consumer compiled against the other version
    /// can no longer read data produced under this one.
    pub fn is_breaking(&self) -> bool {
        match self {
            SchemaChange::FieldAdded { required, .. } => *required,
            SchemaChange::FieldRemoved { required, .. } => *required,
            SchemaChange::FieldTypeChanged { .. } => true,
            SchemaChange::FieldBecameRequired { .. } => true,
            SchemaChange::FieldBecameOptional { .. } => false,
        }
    }

    pub fn describe(&self) -> String {
        match self {
            SchemaChange::FieldAdded { field, required } => {
                format!("added {} field '{}'", if *required { "required" } else { "optional" }, field)
            }
            SchemaChange::FieldRemoved { field, required } => {
                format!("removed {} field '{}'", if *required { "required" } else { "optional" }, field)
            }
            SchemaChange::FieldTypeChanged { field, from, to } => {
                format!("field '{}' changed type from {} to {}", field, from, to)
            }
            SchemaChange::FieldBecameRequired { field } => {
                format!("field '{}' is now required", field)
            }
            SchemaChange::FieldBecameOptional { field } => {
                format!("field '{}' is now optional", field)
            }
        }
    }
}

/// Result of comparing two schema versions.
#[derive(Debug, Serialize)]
pub struct CompatibilityReport {
    pub compatible: bool,
    pub mode: CompatibilityMode,
    pub changes: Vec<SchemaChange>,
    pub breaking_changes: Vec<String>,
    /// Human-readable steps for producers/consumers to migrate.
    pub migration_guidance: Vec<String>,
    /// Fields marked deprecated in the new schema.
    pub deprecation_warnings: Vec<String>,
}

/// Diff two JSON Schemas at the top-level `properties`/`required` level.
pub fn diff_schemas(old: &Value, new: &Value) -> Vec<SchemaChange> {
    let old_props = schema_properties(old);
    let new_props = schema_properties(new);
    let old_required = schema_required(old);
    let new_required = schema_required(new);

    let mut changes = Vec::new();

    for (field, new_type) in &new_props {
        match old_props.get(field) {
            None => changes.push(SchemaChange::FieldAdded {
                field: field.clone(),
                required: new_required.contains(field),
            }),
            Some(old_type) if old_type != new_type => {
                changes.push(SchemaChange::FieldTypeChanged {
                    field: field.clone(),
                    from: old_type.clone(),
                    to: new_type.clone(),
                });
            }
            Some(_) => {}
        }
    }

    for (field, _) in &old_props {
        if !new_props.contains_key(field) {
            changes.push(SchemaChange::FieldRemoved {
                field: field.clone(),
                required: old_required.contains(field),
            });
        }
    }

    for field in &new_required {
        if old_props.contains_key(field) && !old_required.contains(field) {
            changes.push(SchemaChange::FieldBecameRequired {
                field: field.clone(),
            });
        }
    }

    for field in &old_required {
        if new_props.contains_key(field) && !new_required.contains(field) {
            changes.push(SchemaChange::FieldBecameOptional {
                field: field.clone(),
            });
        }
    }

    changes
}

/// Check whether `new` may replace `old` under the given compatibility mode.
pub fn check_compatibility(
    old: &Value,
    new: &Value,
    mode: CompatibilityMode,
) -> CompatibilityReport {
    let changes = diff_schemas(old, new);

    let violates = |change: &SchemaChange| -> bool {
        match mode {
            CompatibilityMode::None => false,
            // Backward: new readers must handle old data, so newly required
            // fields and type changes break them.
            CompatibilityMode::Backward => matches!(
                change,
                SchemaChange::FieldAdded { required: true, .. }
                    | SchemaChange::FieldBecameRequired { .. }
                    | SchemaChange::FieldTypeChanged { .. }
            ),
            // Forward: old readers must handle new data, so removing a field
            // they depend on breaks them.
            CompatibilityMode::Forward => matches!(
                change,
                SchemaChange::FieldRemoved { required: true, .. }
                    | SchemaChange::FieldTypeChanged { .. }
            ),
            CompatibilityMode::Full => change.is_breaking(),
        }
    };

    let breaking_changes: Vec<String> = changes
        .iter()
        .filter(|c| violates(c))
        .map(|c| c.describe())
        .collect();

    let migration_guidance = changes
        .iter()
        .filter(|c| violates(c))
        .map(migration_step)
        .collect();

    CompatibilityReport {
        compatible: breaking_changes.is_empty(),
        mode,
        deprecation_warnings: deprecated_fields(new),
        breaking_changes,
        migration_guidance,
        changes,
    }
}

fn migration_step(change: &SchemaChange) -> String {
    match change {
        SchemaChange::FieldAdded { field, .. } => format!(
            "Give '{}' a default, or ship it as optional first and require it in a later version",
            field
        ),
        SchemaChange::FieldRemoved { field, .. } => format!(
            "Mark '{}' deprecated for one release before removing it",
            field
        ),
        SchemaChange::FieldTypeChanged { field, from, to } => format!(
            "Introduce a new field for the {} form of '{}' instead of changing it from {} in place",
            to, field, from
        ),
        SchemaChange::FieldBecameRequired { field } => format!(
            "Backfill '{}' on existing producers before making it required",
            field
        ),
        SchemaChange::FieldBecameOptional { field } => {
            format!("Consumers of '{}' must tolerate its absence", field)
        }
    }
}

/// Fields carrying a `deprecated: true` annotation in the schema.
fn deprecated_fields(schema: &Value) -> Vec<String> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| {
            props
                .iter()
                .filter(|(_, def)| def.get("deprecated").and_then(|d| d.as_bool()).unwrap_or(false))
                .map(|(name, _)| format!("field '{}' is deprecated and will be removed", name))
                .collect()
        })
        .unwrap_or_default()
}

fn schema_properties(schema: &Value) -> HashMap<String, String> {
    schema
        .get("properties")
        .and_then(|p| p.as_object())
        .map(|props| {
            props
                .iter()
                .map(|(name, def)| {
                    let ty = def
                        .get("type")
                        .map(|t| t.to_string())
                        .unwrap_or_else(|| "any".to_string());
                    (name.clone(), ty)
                })
                .collect()
        })
        .unwrap_or_default()
}

fn schema_required(schema: &Value) -> HashSet<String> {
    schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// One stored revision of a contract's schema.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SchemaVersion {
    pub contract_id: String,
    pub version: i32,
    pub schema: Value,
    pub description: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Compatibility of every stored version against the current one.
#[derive(Debug, Serialize)]
pub struct CompatibilityMatrixEntry {
    pub from_version: i32,
    pub to_version: i32,
    pub compatible: bool,
    pub breaking_changes: Vec<String>,
}

impl SchemaValidator {
    /// Validate honouring the configured mode: lenient accepts invalid events
    /// while still surfacing the errors so producers can be chased down.
    pub async fn validate_with_mode(
        &self,
        contract_id: &str,
        event_data: &Value,
        mode: ValidationMode,
    ) -> Option<ValidationOutcome> {
        let (valid, errors) = self.validate_event_data(contract_id, event_data).await?;
        let accepted = valid || mode == ValidationMode::Lenient;

        if !valid && mode == ValidationMode::Lenient {
            warn!(
                contract_id = %contract_id,
                error_count = errors.len(),
                "Event failed validation but was accepted in lenient mode"
            );
        }

        Some(ValidationOutcome {
            valid,
            accepted,
            mode,
            errors,
        })
    }

    /// Register a new revision, refusing it when it breaks the required
    /// compatibility guarantee against the currently active schema.
    pub async fn register_schema_checked(
        &self,
        contract_id: &str,
        schema: &Value,
        description: Option<&str>,
        mode: CompatibilityMode,
    ) -> Result<CompatibilityReport, anyhow::Error> {
        let report = match self.get_schema(contract_id).await {
            Some(existing) => check_compatibility(&existing, schema, mode),
            None => CompatibilityReport {
                compatible: true,
                mode,
                changes: vec![],
                breaking_changes: vec![],
                migration_guidance: vec![],
                deprecation_warnings: deprecated_fields(schema),
            },
        };

        if !report.compatible {
            return Err(anyhow::anyhow!(
                "Schema for {} is not {:?}-compatible: {}",
                contract_id,
                mode,
                report.breaking_changes.join("; ")
            ));
        }

        register_schema_with_desc(self, contract_id, schema, description).await?;
        self.archive_version(contract_id, schema, description).await?;
        Ok(report)
    }

    /// Append the schema to the version history table.
    pub async fn archive_version(
        &self,
        contract_id: &str,
        schema: &Value,
        description: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            INSERT INTO contract_schema_versions (contract_id, version, schema, description, created_at)
            SELECT $1, COALESCE(MAX(version), 0) + 1, $2, $3, NOW()
            FROM contract_schema_versions WHERE contract_id = $1
            "#,
        )
        .bind(contract_id)
        .bind(schema)
        .bind(description)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// All stored revisions for a contract, oldest first.
    pub async fn list_versions(&self, contract_id: &str) -> Result<Vec<SchemaVersion>, sqlx::Error> {
        sqlx::query_as::<_, SchemaVersion>(
            "SELECT contract_id, version, schema, description, created_at \
             FROM contract_schema_versions WHERE contract_id = $1 ORDER BY version",
        )
        .bind(contract_id)
        .fetch_all(&self.pool)
        .await
    }

    /// Fetch one historical revision.
    pub async fn get_version(&self, contract_id: &str, version: i32) -> Option<SchemaVersion> {
        sqlx::query_as::<_, SchemaVersion>(
            "SELECT contract_id, version, schema, description, created_at \
             FROM contract_schema_versions WHERE contract_id = $1 AND version = $2",
        )
        .bind(contract_id)
        .bind(version)
        .fetch_optional(&self.pool)
        .await
        .ok()
        .flatten()
    }

    /// Compatibility of every earlier revision against the latest one.
    pub async fn compatibility_matrix(
        &self,
        contract_id: &str,
        mode: CompatibilityMode,
    ) -> Result<Vec<CompatibilityMatrixEntry>, sqlx::Error> {
        let versions = self.list_versions(contract_id).await?;
        let Some(latest) = versions.last() else {
            return Ok(vec![]);
        };

        Ok(versions
            .iter()
            .take(versions.len().saturating_sub(1))
            .map(|old| {
                let report = check_compatibility(&old.schema, &latest.schema, mode);
                CompatibilityMatrixEntry {
                    from_version: old.version,
                    to_version: latest.version,
                    compatible: report.compatible,
                    breaking_changes: report.breaking_changes,
                }
            })
            .collect())
    }

    /// Contract ids with a registered schema, for the schema discovery API.
    pub async fn discover(&self, prefix: Option<&str>) -> Result<Vec<SchemaInfo>, sqlx::Error> {
        let all = self.list_schemas().await?;
        Ok(match prefix {
            Some(p) => all
                .into_iter()
                .filter(|s| s.contract_id.starts_with(p))
                .collect(),
            None => all,
        })
    }
}

/// Generate a minimal example document satisfying a schema, for test fixtures
/// and the validation CLI.
pub fn generate_test_data(schema: &Value) -> Value {
    let required = schema_required(schema);
    let mut out = serde_json::Map::new();

    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (name, def) in props {
            if !required.is_empty() && !required.contains(name) {
                continue;
            }
            out.insert(name.clone(), sample_for(def));
        }
    }

    Value::Object(out)
}

fn sample_for(def: &Value) -> Value {
    if let Some(example) = def.get("example") {
        return example.clone();
    }
    if let Some(first) = def.get("enum").and_then(|e| e.as_array()).and_then(|a| a.first()) {
        return first.clone();
    }

    match def.get("type").and_then(|t| t.as_str()) {
        Some("string") => Value::String("example".to_string()),
        Some("integer") => Value::from(0),
        Some("number") => Value::from(0.0),
        Some("boolean") => Value::Bool(false),
        Some("array") => Value::Array(vec![]),
        Some("object") => generate_test_data(def),
        _ => Value::Null,
    }
}

/// Render Markdown documentation for a registered schema.
pub fn document_schema(contract_id: &str, version: i32, schema: &Value) -> String {
    let required = schema_required(schema);
    let mut doc = format!("# Schema: {}\n\nVersion: {}\n\n", contract_id, version);

    if let Some(description) = schema.get("description").and_then(|d| d.as_str()) {
        doc.push_str(&format!("{}\n\n", description));
    }

    doc.push_str("| Field | Type | Required | Notes |\n|---|---|---|---|\n");
    if let Some(props) = schema.get("properties").and_then(|p| p.as_object()) {
        for (name, def) in props {
            let ty = def.get("type").and_then(|t| t.as_str()).unwrap_or("any");
            let req = if required.contains(name) { "yes" } else { "no" };
            let mut notes = def
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            if def.get("deprecated").and_then(|d| d.as_bool()).unwrap_or(false) {
                notes = format!("DEPRECATED. {}", notes);
            }
            doc.push_str(&format!("| {} | {} | {} | {} |\n", name, ty, req, notes.trim()));
        }
    }

    doc
}
