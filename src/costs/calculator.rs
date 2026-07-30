/// Central cost aggregator.
///
/// `CostCalculator` collects per-resource cost entries, computes a
/// combined breakdown, and surfaces efficiency metrics.
use super::{
    compute::{self, ComputeRates, ComputeUsage},
    database::{self, DatabaseRates, DatabaseUsage},
    models::{
        CostBreakdown, CostEntry, CostMetrics, CostReport, ResourceType,
    },
};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Thread-safe, shared cost calculator.
#[derive(Clone)]
pub struct CostCalculator {
    inner: Arc<RwLock<Inner>>,
}

struct Inner {
    entries: Vec<CostEntry>,
    db_rates: DatabaseRates,
    compute_rates: ComputeRates,
    event_count: u64,
    request_count: u64,
}

impl CostCalculator {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner {
                entries: Vec::new(),
                db_rates: DatabaseRates::default(),
                compute_rates: ComputeRates::default(),
                event_count: 0,
                request_count: 0,
            })),
        }
    }

    /// Replace the default database pricing rates.
    pub fn with_db_rates(self, rates: DatabaseRates) -> Self {
        self.inner.write().unwrap().db_rates = rates;
        self
    }

    /// Replace the default compute pricing rates.
    pub fn with_compute_rates(self, rates: ComputeRates) -> Self {
        self.inner.write().unwrap().compute_rates = rates;
        self
    }

    /// Record database cost for the given usage snapshot over `hours`.
    pub fn record_database(&self, usage: &DatabaseUsage, hours: f64) {
        let (db_cost, entry) = {
            let inner = self.inner.read().unwrap();
            let db_cost = database::calculate(usage, &inner.db_rates, hours);
            let entry = database::to_cost_entry(&db_cost, "primary-database");
            (db_cost, entry)
        };
        let _ = db_cost; // suppress unused warning; cost is embedded in entry
        self.inner.write().unwrap().entries.push(entry);
    }

    /// Record compute cost for the given usage snapshot over `hours`.
    pub fn record_compute(&self, usage: &ComputeUsage, hours: f64) {
        let (compute_cost, entry) = {
            let inner = self.inner.read().unwrap();
            let compute_cost = compute::calculate(usage, &inner.compute_rates, hours);
            let entry = compute::to_cost_entry(&compute_cost, "application-server");
            (compute_cost, entry)
        };
        let _ = compute_cost;
        self.inner.write().unwrap().entries.push(entry);
    }

    /// Add an arbitrary pre-computed cost entry (e.g. network, storage).
    pub fn record_entry(&self, entry: CostEntry) {
        self.inner.write().unwrap().entries.push(entry);
    }

    /// Update the running event and request counters (used for efficiency metrics).
    pub fn update_counters(&self, events: u64, requests: u64) {
        let mut inner = self.inner.write().unwrap();
        inner.event_count += events;
        inner.request_count += requests;
    }

    /// Compute a cost breakdown aggregated across all recorded entries.
    pub fn breakdown(&self) -> CostBreakdown {
        let inner = self.inner.read().unwrap();
        build_breakdown(&inner.entries)
    }

    /// Generate a cost report over the entries recorded since the given start time.
    pub fn report(&self, period_start: chrono::DateTime<Utc>) -> CostReport {
        let inner = self.inner.read().unwrap();
        let period_entries: Vec<CostEntry> = inner
            .entries
            .iter()
            .filter(|e| e.timestamp >= period_start)
            .cloned()
            .collect();

        let breakdown = build_breakdown(&period_entries);

        let mut by_resource_type: HashMap<ResourceType, f64> = HashMap::new();
        for e in &period_entries {
            *by_resource_type.entry(e.resource_type).or_insert(0.0) += e.cost;
        }

        let mut resource_totals: HashMap<&str, f64> = HashMap::new();
        for e in &period_entries {
            *resource_totals.entry(e.resource_id.as_str()).or_insert(0.0) += e.cost;
        }
        let mut top_resources: Vec<(String, f64)> = resource_totals
            .into_iter()
            .map(|(id, cost)| (id.to_string(), cost))
            .collect();
        top_resources.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        top_resources.truncate(10);

        CostReport {
            period_start,
            period_end: Utc::now(),
            breakdown,
            by_resource_type,
            top_resources,
        }
    }

    /// Compute efficiency metrics (cost-per-event, cost-per-request, etc.).
    pub fn metrics(&self) -> CostMetrics {
        let inner = self.inner.read().unwrap();
        let breakdown = build_breakdown(&inner.entries);
        let total = breakdown.total;

        let cost_per_event = if inner.event_count > 0 {
            total / inner.event_count as f64
        } else {
            0.0
        };

        let cost_per_request = if inner.request_count > 0 {
            total / inner.request_count as f64
        } else {
            0.0
        };

        // Derive hourly rate from the time span of recorded entries.
        let hourly_rate = hourly_rate_from_entries(&inner.entries);

        CostMetrics {
            total_cost: total,
            cost_per_event,
            cost_per_request,
            hourly_rate,
            daily_average: hourly_rate * 24.0,
        }
    }

    /// Remove all entries older than `retain_hours` hours.
    pub fn prune_older_than(&self, retain_hours: i64) {
        let cutoff = Utc::now() - chrono::Duration::hours(retain_hours);
        self.inner
            .write()
            .unwrap()
            .entries
            .retain(|e| e.timestamp >= cutoff);
    }

    /// Total number of recorded cost entries.
    pub fn entry_count(&self) -> usize {
        self.inner.read().unwrap().entries.len()
    }
}

impl Default for CostCalculator {
    fn default() -> Self {
        Self::new()
    }
}

// ── helpers ──────────────────────────────────────────────────────────────────

fn build_breakdown(entries: &[CostEntry]) -> CostBreakdown {
    let mut database = 0.0f64;
    let mut compute = 0.0f64;
    let mut storage = 0.0f64;
    let mut network = 0.0f64;
    let mut details = HashMap::new();

    for entry in entries {
        match entry.resource_type {
            ResourceType::Database => database += entry.cost,
            ResourceType::Compute | ResourceType::Memory => compute += entry.cost,
            ResourceType::Storage => storage += entry.cost,
            ResourceType::Network => network += entry.cost,
        }
        // Keep the latest entry per resource_id for the details map.
        details.insert(entry.resource_id.clone(), entry.clone());
    }

    let total = database + compute + storage + network;

    CostBreakdown {
        timestamp: Utc::now(),
        database,
        compute,
        storage,
        network,
        total,
        details,
    }
}

fn hourly_rate_from_entries(entries: &[CostEntry]) -> f64 {
    if entries.is_empty() {
        return 0.0;
    }
    let oldest = entries.iter().map(|e| e.timestamp).min().unwrap();
    let hours = (Utc::now() - oldest).num_minutes() as f64 / 60.0;
    if hours < 0.001 {
        return 0.0;
    }
    let total: f64 = entries.iter().map(|e| e.cost).sum();
    total / hours
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::costs::database::DatabaseUsage;
    use crate::costs::compute::ComputeUsage;

    #[test]
    fn empty_calculator_yields_zero_breakdown() {
        let calc = CostCalculator::new();
        let b = calc.breakdown();
        assert_eq!(b.total, 0.0);
    }

    #[test]
    fn records_database_and_compute() {
        let calc = CostCalculator::new();
        calc.record_database(
            &DatabaseUsage { query_count: 1000, active_connections: 1, ..Default::default() },
            1.0,
        );
        calc.record_compute(
            &ComputeUsage { vcpu_count: 1, cpu_utilization: 0.5, ..Default::default() },
            1.0,
        );
        let b = calc.breakdown();
        assert!(b.database > 0.0);
        assert!(b.compute > 0.0);
        assert!(b.total > 0.0);
    }
}
