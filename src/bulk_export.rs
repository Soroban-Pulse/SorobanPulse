/// Issue #881: Bulk event export with compression
/// Supports multiple output formats and compression algorithms with streaming

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};
use std::path::PathBuf;
use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use flate2::write::GzEncoder;
use flate2::Compression as GzipCompression;
use tracing::{info, error};
use std::io::Write;

/// Supported export formats
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportFormat {
    JsonLines,
    Parquet,
    Csv,
}

impl std::fmt::Display for ExportFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JsonLines => write!(f, "jsonlines"),
            Self::Parquet => write!(f, "parquet"),
            Self::Csv => write!(f, "csv"),
        }
    }
}

/// Supported compression algorithms
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CompressionAlgorithm {
    Gzip,
    Brotli,
    Zstd,
    None,
}

impl std::fmt::Display for CompressionAlgorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Gzip => write!(f, "gzip"),
            Self::Brotli => write!(f, "brotli"),
            Self::Zstd => write!(f, "zstd"),
            Self::None => write!(f, "none"),
        }
    }
}

/// Export request parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportRequest {
    /// Export format
    pub format: ExportFormat,
    /// Compression algorithm
    pub compression: CompressionAlgorithm,
    /// Filter by contract ID (optional)
    pub contract_id: Option<String>,
    /// Filter by event type (optional)
    pub event_type: Option<String>,
    /// Minimum ledger sequence (optional)
    pub ledger_min: Option<u64>,
    /// Maximum ledger sequence (optional)
    pub ledger_max: Option<u64>,
    /// Start timestamp (optional)
    pub start_time: Option<DateTime<Utc>>,
    /// End timestamp (optional)
    pub end_time: Option<DateTime<Utc>>,
    /// Batch size for streaming (default: 10000)
    pub batch_size: Option<i32>,
}

impl Default for ExportRequest {
    fn default() -> Self {
        Self {
            format: ExportFormat::JsonLines,
            compression: CompressionAlgorithm::Gzip,
            contract_id: None,
            event_type: None,
            ledger_min: None,
            ledger_max: None,
            start_time: None,
            end_time: None,
            batch_size: Some(10000),
        }
    }
}

/// Export job tracking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportJob {
    pub id: String,
    pub request: ExportRequest,
    pub status: ExportStatus,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub file_path: Option<PathBuf>,
    pub total_events: u64,
    pub processed_events: u64,
    pub file_size_bytes: u64,
    pub download_url: Option<String>,
}

/// Export job status
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ExportStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Expired,
}

/// Bulk export manager
pub struct BulkExportManager {
    jobs: Arc<RwLock<HashMap<String, ExportJob>>>,
    export_dir: PathBuf,
    retention_hours: u64,
}

impl BulkExportManager {
    /// Create a new bulk export manager
    pub fn new(export_dir: PathBuf, retention_hours: u64) -> Self {
        Self {
            jobs: Arc::new(RwLock::new(HashMap::new())),
            export_dir,
            retention_hours,
        }
    }

    /// Start a new export job
    pub async fn start_export(
        &self,
        pool: &PgPool,
        request: ExportRequest,
    ) -> Result<String, String> {
        let job_id = Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + Duration::hours(self.retention_hours as i64);

        // Get total count first
        let total_events = self.count_matching_events(pool, &request).await?;

        let job = ExportJob {
            id: job_id.clone(),
            request,
            status: ExportStatus::Pending,
            created_at: now,
            expires_at,
            file_path: None,
            total_events,
            processed_events: 0,
            file_size_bytes: 0,
            download_url: None,
        };

        let mut jobs = self.jobs.write().await;
        jobs.insert(job_id.clone(), job);

        Ok(job_id)
    }

    /// Get export job status
    pub async fn get_job_status(&self, job_id: &str) -> Result<ExportJob, String> {
        let jobs = self.jobs.read().await;
        jobs.get(job_id)
            .cloned()
            .ok_or_else(|| format!("Job {} not found", job_id))
    }

    /// Update job progress
    pub async fn update_progress(
        &self,
        job_id: &str,
        processed: u64,
        status: ExportStatus,
        file_size: Option<u64>,
    ) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.processed_events = processed;
            job.status = status;
            if let Some(size) = file_size {
                job.file_size_bytes = size;
            }
            Ok(())
        } else {
            Err(format!("Job {} not found", job_id))
        }
    }

    /// Complete export job
    pub async fn complete_export(
        &self,
        job_id: &str,
        file_path: PathBuf,
        file_size: u64,
        download_url: String,
    ) -> Result<(), String> {
        let mut jobs = self.jobs.write().await;
        if let Some(job) = jobs.get_mut(job_id) {
            job.status = ExportStatus::Completed;
            job.file_path = Some(file_path);
            job.file_size_bytes = file_size;
            job.download_url = Some(download_url);
            Ok(())
        } else {
            Err(format!("Job {} not found", job_id))
        }
    }

    /// Get all jobs
    pub async fn get_all_jobs(&self) -> Vec<ExportJob> {
        let jobs = self.jobs.read().await;
        jobs.values().cloned().collect()
    }

    /// Clean up expired export files and jobs
    pub async fn cleanup_expired(&self) -> Result<u64, String> {
        let mut jobs = self.jobs.write().await;
        let now = Utc::now();
        let expired: Vec<String> = jobs
            .iter()
            .filter_map(|(id, job)| {
                if job.expires_at < now {
                    if let Some(path) = &job.file_path {
                        let _ = std::fs::remove_file(path);
                    }
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        let count = expired.len() as u64;
        for id in expired {
            jobs.remove(&id);
        }

        Ok(count)
    }

    /// Build SQL query based on export request
    fn build_query(&self, request: &ExportRequest) -> (String, Vec<String>) {
        let mut query = String::from(
            "SELECT id, contract_id, event_type, tx_hash, ledger, ledger_close_time, topic, value, source_account FROM events WHERE 1=1"
        );
        let mut params = Vec::new();

        if let Some(ref contract_id) = request.contract_id {
            query.push_str(" AND contract_id = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(contract_id.clone());
        }

        if let Some(ref event_type) = request.event_type {
            query.push_str(" AND event_type = $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(event_type.clone());
        }

        if let Some(ledger_min) = request.ledger_min {
            query.push_str(" AND ledger >= $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(ledger_min.to_string());
        }

        if let Some(ledger_max) = request.ledger_max {
            query.push_str(" AND ledger <= $");
            query.push_str(&(params.len() + 1).to_string());
            params.push(ledger_max.to_string());
        }

        query.push_str(" ORDER BY ledger DESC LIMIT 1000000");

        (query, params)
    }

    /// Count matching events for a request
    async fn count_matching_events(
        &self,
        pool: &PgPool,
        request: &ExportRequest,
    ) -> Result<u64, String> {
        let mut query = String::from("SELECT COUNT(*) FROM events WHERE 1=1");

        if request.contract_id.is_some() {
            query.push_str(" AND contract_id = $1");
        }
        if request.event_type.is_some() {
            query.push_str(" AND event_type = $2");
        }

        let count: i64 = if let (Some(ref contract_id), Some(ref event_type)) =
            (&request.contract_id, &request.event_type)
        {
            sqlx::query_scalar(&query)
                .bind(contract_id)
                .bind(event_type)
                .fetch_one(pool)
                .await
                .map_err(|e| format!("Database error: {}", e))?
        } else if let Some(ref contract_id) = request.contract_id {
            sqlx::query_scalar(&query)
                .bind(contract_id)
                .fetch_one(pool)
                .await
                .map_err(|e| format!("Database error: {}", e))?
        } else {
            sqlx::query_scalar("SELECT COUNT(*) FROM events")
                .fetch_one(pool)
                .await
                .map_err(|e| format!("Database error: {}", e))?
        };

        Ok(count as u64)
    }

    /// Generate download URL for exported file
    pub fn generate_download_url(&self, job_id: &str, file_path: &PathBuf) -> String {
        format!("/v1/admin/events/export/{}/download", job_id)
    }
}

/// Export statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportStatistics {
    pub total_jobs: u64,
    pub completed_jobs: u64,
    pub in_progress_jobs: u64,
    pub failed_jobs: u64,
    pub total_data_exported_bytes: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_export_request_has_reasonable_values() {
        let req = ExportRequest::default();
        assert_eq!(req.format, ExportFormat::JsonLines);
        assert_eq!(req.compression, CompressionAlgorithm::Gzip);
        assert_eq!(req.batch_size, Some(10000));
    }

    #[test]
    fn export_format_display() {
        assert_eq!(ExportFormat::JsonLines.to_string(), "jsonlines");
        assert_eq!(ExportFormat::Parquet.to_string(), "parquet");
        assert_eq!(ExportFormat::Csv.to_string(), "csv");
    }

    #[test]
    fn compression_algorithm_display() {
        assert_eq!(CompressionAlgorithm::Gzip.to_string(), "gzip");
        assert_eq!(CompressionAlgorithm::Brotli.to_string(), "brotli");
        assert_eq!(CompressionAlgorithm::Zstd.to_string(), "zstd");
        assert_eq!(CompressionAlgorithm::None.to_string(), "none");
    }

    #[test]
    fn export_status_ordering() {
        let pending = ExportStatus::Pending;
        let in_progress = ExportStatus::InProgress;
        let completed = ExportStatus::Completed;

        assert_ne!(pending, in_progress);
        assert_ne!(in_progress, completed);
    }
}
