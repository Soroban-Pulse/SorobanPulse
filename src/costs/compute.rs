/// Compute cost tracking.
///
/// Tracks CPU usage and translates it into monetary cost based on
/// configurable vCPU-hour pricing.
use super::models::{ComputeCost, CostEntry, ResourceType};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Pricing rates for compute resources (USD).
#[derive(Debug, Clone)]
pub struct ComputeRates {
    /// Cost per vCPU-hour.
    pub vcpu_hour: f64,
    /// Cost per GB of memory per hour.
    pub memory_gb_hour: f64,
}

impl Default for ComputeRates {
    fn default() -> Self {
        Self {
            vcpu_hour: 0.048,
            memory_gb_hour: 0.006,
        }
    }
}

/// Snapshot of observed compute usage during a measurement window.
#[derive(Debug, Clone, Default)]
pub struct ComputeUsage {
    pub vcpu_count: u32,
    /// CPU utilisation [0.0, 1.0].
    pub cpu_utilization: f64,
    pub memory_bytes: u64,
    pub request_count: u64,
    pub worker_threads: u32,
}

/// Shared, thread-safe request counter for the compute tracker.
#[derive(Clone, Default)]
pub struct RequestCounter(Arc<AtomicU64>);

impl RequestCounter {
    pub fn new() -> Self {
        Self(Arc::new(AtomicU64::new(0)))
    }

    pub fn increment(&self) {
        self.0.fetch_add(1, Ordering::Relaxed);
    }

    /// Drain the counter and return the total since the last drain.
    pub fn drain(&self) -> u64 {
        self.0.swap(0, Ordering::Relaxed)
    }

    pub fn current(&self) -> u64 {
        self.0.load(Ordering::Relaxed)
    }
}

/// Calculate compute cost from a usage snapshot and an elapsed window (in hours).
pub fn calculate(usage: &ComputeUsage, rates: &ComputeRates, hours: f64) -> ComputeCost {
    // Effective CPU hours = vCPUs × utilisation × elapsed hours.
    let cpu_hours = f64::from(usage.vcpu_count) * usage.cpu_utilization.clamp(0.0, 1.0) * hours;
    let total_cost = cpu_hours * rates.vcpu_hour;

    ComputeCost {
        cpu_hours,
        vcpu_count: usage.vcpu_count,
        total_cost,
    }
}

/// Wrap a `ComputeCost` as a `CostEntry` for aggregation.
pub fn to_cost_entry(compute_cost: &ComputeCost, resource_id: &str) -> CostEntry {
    let mut metadata = HashMap::new();
    metadata.insert("cpu_hours".into(), format!("{:.4}", compute_cost.cpu_hours));
    metadata.insert("vcpu_count".into(), compute_cost.vcpu_count.to_string());

    CostEntry {
        resource_id: resource_id.to_string(),
        resource_type: ResourceType::Compute,
        cost: compute_cost.total_cost,
        timestamp: Utc::now(),
        metadata,
    }
}

/// Read the current process CPU utilisation on Linux via `/proc/self/stat`.
/// Returns `None` on non-Linux platforms or on read errors.
#[cfg(target_os = "linux")]
pub fn current_cpu_utilization() -> Option<f64> {
    use std::fs;
    use std::time::{Duration, Instant};

    fn read_process_ticks() -> Option<u64> {
        let stat = fs::read_to_string("/proc/self/stat").ok()?;
        let fields: Vec<&str> = stat.split_whitespace().collect();
        // utime is field 14, stime is field 15 (0-indexed).
        let utime: u64 = fields.get(13)?.parse().ok()?;
        let stime: u64 = fields.get(14)?.parse().ok()?;
        Some(utime + stime)
    }

    let before = read_process_ticks()?;
    let t0 = Instant::now();
    std::thread::sleep(Duration::from_millis(100));
    let after = read_process_ticks()?;
    let elapsed = t0.elapsed().as_secs_f64();

    let ticks_per_sec = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let cpu_seconds = (after - before) as f64 / ticks_per_sec;
    Some((cpu_seconds / elapsed).clamp(0.0, 1.0))
}

#[cfg(not(target_os = "linux"))]
pub fn current_cpu_utilization() -> Option<f64> {
    None
}

/// Read current process RSS memory in bytes from `/proc/self/status` (Linux only).
#[cfg(target_os = "linux")]
pub fn current_memory_bytes() -> Option<u64> {
    use std::fs;
    let status = fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if line.starts_with("VmRSS:") {
            let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            return Some(kb * 1024);
        }
    }
    None
}

#[cfg(not(target_os = "linux"))]
pub fn current_memory_bytes() -> Option<u64> {
    None
}

/// Number of vCPUs available to the process.
pub fn available_vcpus() -> u32 {
    u32::try_from(num_cpus()).unwrap_or(1)
}

fn num_cpus() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_utilization_yields_zero_cost() {
        let usage = ComputeUsage { vcpu_count: 4, cpu_utilization: 0.0, ..Default::default() };
        let result = calculate(&usage, &ComputeRates::default(), 1.0);
        assert_eq!(result.total_cost, 0.0);
    }

    #[test]
    fn full_utilization_cost() {
        let rates = ComputeRates { vcpu_hour: 0.048, ..Default::default() };
        let usage = ComputeUsage { vcpu_count: 2, cpu_utilization: 1.0, ..Default::default() };
        let result = calculate(&usage, &rates, 1.0);
        // 2 vCPUs × 1.0 × 1h × $0.048 = $0.096
        assert!((result.total_cost - 0.096).abs() < 1e-10);
    }

    #[test]
    fn request_counter_drains_correctly() {
        let counter = RequestCounter::new();
        counter.increment();
        counter.increment();
        counter.increment();
        assert_eq!(counter.drain(), 3);
        assert_eq!(counter.current(), 0);
    }
}
