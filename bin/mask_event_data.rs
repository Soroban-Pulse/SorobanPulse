#!/usr/bin/env rust-script
//! ```cargo
//! [dependencies]
//! sqlx = { version = "0.7", features = ["runtime-tokio-native-tls", "postgres", "json"] }
//! tokio = { version = "1", features = ["full"] }
//! serde_json = "1.0"
//! serde = { version = "1.0", features = ["derive"] }
//! tracing = "0.1"
//! tracing-subscriber = { version = "0.3", features = ["env-filter"] }
//! anyhow = "1.0"
//! ```

use anyhow::{Context, Result};
use serde_json::{Value, json};
use sqlx::{postgres::{PgPoolOptions, PgRow}, Row, Executor};
use std::collections::HashMap;
use std::env;
use tracing::{info, warn, debug};
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Data masker for sensitive event information
struct DataMasker {
    seed: String,
}

impl DataMasker {
    fn new(seed: Option<String>) -> Self {
        Self {
            seed: seed.unwrap_or_else(|| "sorobanpulse-masking-salt-2024".to_string()),
        }
    }

    /// Generate deterministic hash for a value
    fn deterministic_hash(&self, value: &str, mask_type: &str) -> u64 {
        let mut hasher = DefaultHasher::new();
        format!("{}:{}:{}", self.seed, mask_type, value).hash(&mut hasher);
        hasher.finish()
    }

    /// Mask a stellar address (preserves G/C/M prefix, length ~56)
    fn mask_address(&self, address: &str) -> String {
        if address.is_empty() {
            return address.to_string();
        }
        
        let prefix = match address.chars().next() {
            Some(c) if c == 'G' || c == 'C' || c == 'M' => c,
            _ => 'G',
        };
        
        let hash = self.deterministic_hash(address, "address");
        format!("{}{:x}", prefix, hash)[..56].to_string()
    }

    /// Mask numeric values (preserves approximate scale)
    fn mask_amount(&self, amount: &str) -> String {
        if let Ok(num) = amount.parse::<f64>() {
            let hash = self.deterministic_hash(amount, "amount");
            let scale = ((hash % 100) as f64 / 100.0) * 1.5 + 0.5; // 0.5 to 2.0 scale
            let masked = num * scale;
            format!("{:.7}", masked)
        } else {
            let hash = self.deterministic_hash(amount, "amount");
            format!("{:x}", hash)[..10].to_string()
        }
    }

    /// Mask ID fields
    fn mask_id(&self, id: &str) -> String {
        let hash = self.deterministic_hash(id, "id");
        format!("{:x}", hash)[..16].to_string()
    }

    /// Check if a string appears to be already masked (contains hex pattern)
    fn is_likely_masked(&self, value: &str) -> bool {
        value.len() >= 40 && value.chars().all(|c| c.is_ascii_hexdigit() || c == 'G' || c == 'C' || c == 'M')
    }

    /// Recursively mask event data JSON
    fn mask_event_data(&self, data: &Value) -> Value {
        match data {
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (key, value) in obj {
                    let masked_value = self.mask_event_data(value);
                    
                    // Check if this field should be masked
                    let key_lower = key.to_lowercase();
                    if key_lower.contains("address") || key_lower.contains("account") {
                        if let Value::String(s) = &masked_value {
                            if !self.is_likely_masked(s) {
                                new_obj.insert(key.clone(), Value::String(self.mask_address(s)));
                                continue;
                            }
                        }
                    } else if key_lower.contains("amount") || key_lower.contains("value") {
                        if let Value::String(s) = &masked_value {
                            if !s.chars().all(|c| c.is_ascii_hexdigit()) {
                                new_obj.insert(key.clone(), Value::String(self.mask_amount(s)));
                                continue;
                            }
                        }
                    } else if key_lower.contains("id") && !key_lower.contains("event") {
                        if let Value::String(s) = &masked_value {
                            if !self.is_likely_masked(s) {
                                new_obj.insert(key.clone(), Value::String(self.mask_id(s)));
                                continue;
                            }
                        }
                    }
                    
                    new_obj.insert(key.clone(), masked_value);
                }
                Value::Object(new_obj)
            }
            Value::Array(arr) => {
                Value::Array(arr.iter().map(|v| self.mask_event_data(v)).collect())
            }
            _ => data.clone(),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("info")
        .init();
    
    // Parse arguments
    let db_url = env::var("DATABASE_URL")
        .context("DATABASE_URL environment variable not set")?;
    
    let dry_run = env::args().any(|arg| arg == "--dry-run");
    let table_name = env::var("TABLE_NAME").unwrap_or_else(|_| "events".to_string());
    
    if dry_run {
        info!("DRY RUN MODE - No changes will be made");
    }
    
    // Connect to database
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_url)
        .await
        .context("Failed to connect to database")?;
    
    // Check if we're in production
    let env_filter = env::var("ENVIRONMENT").unwrap_or_else(|_| "development".to_string());
    if env_filter == "production" {
        warn!("⚠️  PRODUCTION ENVIRONMENT DETECTED!");
        let mut input = String::new();
        println!("Type 'YES' to continue masking in production: ");
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "YES" {
            info!("Aborting...");
            return Ok(());
        }
    }
    
    // Get total count
    let total: i64 = sqlx::query_scalar(&format!(
        "SELECT COUNT(*) FROM {} WHERE event_data IS NOT NULL",
        table_name
    ))
    .fetch_one(&pool)
    .await?;
    
    info!("Found {} rows to process", total);
    
    // Process in batches
    let masker = DataMasker::new(None);
    let batch_size: i64 = 100;
    let mut offset = 0;
    let mut masked_count = 0;
    let mut skipped_count = 0;
    
    while offset < total {
        let rows: Vec<(i64, Value)> = sqlx::query_as(&format!(
            "SELECT id, event_data FROM {} WHERE event_data IS NOT NULL ORDER BY id LIMIT $1 OFFSET $2",
            table_name
        ))
        .bind(batch_size)
        .bind(offset)
        .fetch_all(&pool)
        .await?;
        
        for (id, event_data) in rows {
            // Skip if data appears already masked
            let data_str = event_data.to_string();
            if data_str.len() > 40 && masker.is_likely_masked(&data_str[..40]) {
                skipped_count += 1;
                continue;
            }
            
            // Mask the data
            let masked_data = masker.mask_event_data(&event_data);
            
            if !dry_run {
                sqlx::query(&format!(
                    "UPDATE {} SET event_data = $1 WHERE id = $2",
                    table_name
                ))
                .bind(&masked_data)
                .bind(id)
                .execute(&pool)
                .await?;
            }
            
            masked_count += 1;
            
            if masked_count % 100 == 0 {
                info!("Processed {}/{} rows", masked_count + skipped_count, total);
            }
        }
        
        offset += batch_size;
    }
    
    info!("=".repeat(50));
    info!("Masking Complete!");
    info!("Total rows processed: {}", masked_count + skipped_count);
    info!("Rows masked: {}", masked_count);
    info!("Rows skipped (already masked): {}", skipped_count);
    
    if dry_run {
        info!("\nThis was a dry run. Run without --dry-run to apply changes.");
    }
    
    Ok(())
}
