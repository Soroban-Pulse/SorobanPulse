//! Issue #266: Bloom filter deduplication pre-filter.
//!
//! Stores hashes of `(tx_hash, contract_id, event_type)` tuples to skip
//! database inserts for events that are very likely already indexed.
//! False positives cause a missed insert (the DB unique constraint is the
//! authoritative guard); false negatives are impossible by design.

use bloomfilter::Bloom;
use std::sync::Mutex;

use crate::metrics;

/// Thread-safe bloom filter for event deduplication.
pub struct EventBloomFilter {
    inner: Mutex<Bloom<String>>,
}

impl EventBloomFilter {
    /// Create a new bloom filter with the given false-positive rate and capacity.
    ///
    /// # Panics
    /// Panics if `fp_rate` is not in (0, 1) or `capacity` is 0.
    pub fn new(capacity: usize, fp_rate: f64) -> Self {
        let bloom = Bloom::new_for_fp_rate(capacity, fp_rate)
            .expect("Failed to create bloom filter: invalid capacity or fp_rate");
        Self {
            inner: Mutex::new(bloom),
        }
    }

    /// Build the deduplication key for an event.
    fn key(tx_hash: &str, contract_id: &str, event_type: &str) -> String {
        format!("{tx_hash}:{contract_id}:{event_type}")
    }

    /// Returns `true` if the event was probably already seen (bloom filter hit).
    /// Increments `soroban_pulse_bloom_filter_hits_total` on a hit.
    pub fn check(&self, tx_hash: &str, contract_id: &str, event_type: &str) -> bool {
        let k = Self::key(tx_hash, contract_id, event_type);
        let hit = self
            .inner
            .lock()
            .expect("bloom filter lock poisoned")
            .check(&k);
        if hit {
            metrics::record_bloom_filter_hit();
        }
        hit
    }

    /// Record that an event has been seen.
    pub fn set(&self, tx_hash: &str, contract_id: &str, event_type: &str) {
        let k = Self::key(tx_hash, contract_id, event_type);
        self.inner
            .lock()
            .expect("bloom filter lock poisoned")
            .set(&k);
    }

    /// Seed the filter from a list of `(tx_hash, contract_id, event_type)` tuples.
    /// Used at startup to pre-populate from recent DB rows.
    pub fn seed(&self, entries: impl IntoIterator<Item = (String, String, String)>) {
        let mut guard = self.inner.lock().expect("bloom filter lock poisoned");
        for (tx_hash, contract_id, event_type) in entries {
            let k = Self::key(&tx_hash, &contract_id, &event_type);
            guard.set(&k);
        }
    }
}

/// Load recent events from the database and seed the bloom filter.
/// Loads up to `limit` most recent events by ledger descending.
pub async fn seed_from_db(
    filter: &EventBloomFilter,
    pool: &sqlx::PgPool,
    limit: i64,
) -> Result<usize, sqlx::Error> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT tx_hash, contract_id, event_type FROM events ORDER BY ledger DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    let count = rows.len();
    filter.seed(rows.into_iter().map(|(tx, cid, et)| (tx, cid, et)));
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_filter() -> EventBloomFilter {
        EventBloomFilter::new(10_000, 0.001)
    }

    #[test]
    fn new_filter_has_no_hits() {
        let f = make_filter();
        assert!(!f.check("tx1", "contract1", "contract"));
    }

    #[test]
    fn set_then_check_returns_true() {
        let f = make_filter();
        f.set("tx1", "contract1", "contract");
        assert!(f.check("tx1", "contract1", "contract"));
    }

    #[test]
    fn different_event_type_not_hit() {
        let f = make_filter();
        f.set("tx1", "contract1", "contract");
        assert!(!f.check("tx1", "contract1", "system"));
    }

    #[test]
    fn different_tx_hash_not_hit() {
        let f = make_filter();
        f.set("tx1", "contract1", "contract");
        assert!(!f.check("tx2", "contract1", "contract"));
    }

    #[test]
    fn seed_populates_filter() {
        let f = make_filter();
        f.seed(vec![
            ("tx1".into(), "c1".into(), "contract".into()),
            ("tx2".into(), "c2".into(), "system".into()),
        ]);
        assert!(f.check("tx1", "c1", "contract"));
        assert!(f.check("tx2", "c2", "system"));
        assert!(!f.check("tx3", "c3", "contract"));
    }

    #[test]
    fn multiple_sets_all_hit() {
        let f = make_filter();
        for i in 0..100u32 {
            f.set(&format!("tx{i}"), "contract1", "contract");
        }
        for i in 0..100u32 {
            assert!(f.check(&format!("tx{i}"), "contract1", "contract"));
        }
    }
}
