//! HTTP/3 (QUIC) protocol support with negotiation, connection persistence and metrics.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Supported application protocols, in negotiation preference order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    #[serde(rename = "h3")]
    Http3,
    #[serde(rename = "h2")]
    Http2,
    #[serde(rename = "http/1.1")]
    Http1_1,
}

impl Protocol {
    pub fn alpn_id(&self) -> &'static str {
        match self {
            Protocol::Http3 => "h3",
            Protocol::Http2 => "h2",
            Protocol::Http1_1 => "http/1.1",
        }
    }
}

/// Configuration for enabling HTTP/3 (QUIC) on the web server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Http3Config {
    pub enabled: bool,
    pub udp_bind_addr: String,
    pub udp_port: u16,
    /// Advertised Alt-Svc max-age in seconds, telling clients how long to remember
    /// that this origin supports HTTP/3.
    pub alt_svc_max_age_secs: u64,
    /// Maximum idle time before a QUIC connection is closed.
    pub max_idle_timeout_ms: u64,
    /// Maximum number of concurrent bidirectional streams per connection.
    pub max_concurrent_streams: u32,
    /// Whether 0-RTT session resumption is permitted.
    pub allow_0rtt: bool,
    pub cert_path: String,
    pub key_path: String,
}

impl Default for Http3Config {
    fn default() -> Self {
        Self {
            enabled: false,
            udp_bind_addr: "0.0.0.0".to_string(),
            udp_port: 443,
            alt_svc_max_age_secs: 86400,
            max_idle_timeout_ms: 30_000,
            max_concurrent_streams: 100,
            allow_0rtt: true,
            cert_path: "certs/fullchain.pem".to_string(),
            key_path: "certs/privkey.pem".to_string(),
        }
    }
}

impl Http3Config {
    /// Builds the `Alt-Svc` header value advertising HTTP/3 support for protocol negotiation.
    pub fn alt_svc_header(&self) -> String {
        format!(
            "h3=\":{}\"; ma={}",
            self.udp_port, self.alt_svc_max_age_secs
        )
    }
}

/// Result of negotiating a protocol against a client's advertised ALPN list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NegotiationResult {
    pub selected: Protocol,
    pub client_offered: Vec<String>,
    pub fell_back: bool,
}

/// Negotiates the best mutually supported protocol from a client's ALPN offer list.
pub fn negotiate_protocol(client_alpn: &[String], http3_enabled: bool) -> NegotiationResult {
    let preference = if http3_enabled {
        [Protocol::Http3, Protocol::Http2, Protocol::Http1_1]
    } else {
        [Protocol::Http2, Protocol::Http1_1, Protocol::Http1_1]
    };

    for candidate in preference {
        if client_alpn.iter().any(|p| p == candidate.alpn_id()) {
            return NegotiationResult {
                selected: candidate,
                client_offered: client_alpn.to_vec(),
                fell_back: candidate != Protocol::Http3,
            };
        }
    }

    NegotiationResult {
        selected: Protocol::Http1_1,
        client_offered: client_alpn.to_vec(),
        fell_back: true,
    }
}

/// Tracks persisted QUIC connections so subsequent requests from the same client
/// can reuse an established connection instead of paying handshake cost again.
#[derive(Debug, Default)]
pub struct ConnectionPersistenceTracker {
    connections: HashMap<String, PersistedConnection>,
}

#[derive(Debug, Clone)]
struct PersistedConnection {
    established_at: Instant,
    last_used: Instant,
    requests_served: u64,
}

impl ConnectionPersistenceTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record_connection(&mut self, client_key: impl Into<String>) {
        let now = Instant::now();
        self.connections
            .entry(client_key.into())
            .and_modify(|c| {
                c.last_used = now;
                c.requests_served += 1;
            })
            .or_insert(PersistedConnection {
                established_at: now,
                last_used: now,
                requests_served: 1,
            });
    }

    pub fn is_persisted(&self, client_key: &str) -> bool {
        self.connections.contains_key(client_key)
    }

    /// Evicts connections that have been idle beyond `max_idle`.
    pub fn evict_idle(&mut self, max_idle: Duration) -> usize {
        let now = Instant::now();
        let before = self.connections.len();
        self.connections
            .retain(|_, c| now.duration_since(c.last_used) < max_idle);
        before - self.connections.len()
    }

    pub fn active_connections(&self) -> usize {
        self.connections.len()
    }
}

/// Protocol-level metrics for HTTP/3 vs. HTTP/2 usage, shared across request handlers.
#[derive(Debug, Default)]
pub struct Http3Metrics {
    pub http3_requests: AtomicU64,
    pub http2_requests: AtomicU64,
    pub http1_requests: AtomicU64,
    pub protocol_downgrades: AtomicU64,
    pub zero_rtt_accepted: AtomicU64,
    pub zero_rtt_rejected: AtomicU64,
}

impl Http3Metrics {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn record_negotiation(&self, result: &NegotiationResult) {
        match result.selected {
            Protocol::Http3 => self.http3_requests.fetch_add(1, Ordering::Relaxed),
            Protocol::Http2 => self.http2_requests.fetch_add(1, Ordering::Relaxed),
            Protocol::Http1_1 => self.http1_requests.fetch_add(1, Ordering::Relaxed),
        };
        if result.fell_back {
            self.protocol_downgrades.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn record_zero_rtt(&self, accepted: bool) {
        if accepted {
            self.zero_rtt_accepted.fetch_add(1, Ordering::Relaxed);
        } else {
            self.zero_rtt_rejected.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// Renders metrics in Prometheus exposition format.
    pub fn to_prometheus(&self) -> String {
        format!(
            "# HELP sorobanpulse_http3_requests_total Requests served over HTTP/3\n\
             # TYPE sorobanpulse_http3_requests_total counter\n\
             sorobanpulse_http3_requests_total {}\n\
             # HELP sorobanpulse_http2_requests_total Requests served over HTTP/2\n\
             # TYPE sorobanpulse_http2_requests_total counter\n\
             sorobanpulse_http2_requests_total {}\n\
             # HELP sorobanpulse_http1_requests_total Requests served over HTTP/1.1\n\
             # TYPE sorobanpulse_http1_requests_total counter\n\
             sorobanpulse_http1_requests_total {}\n\
             # HELP sorobanpulse_protocol_downgrades_total Times negotiation fell back from HTTP/3\n\
             # TYPE sorobanpulse_protocol_downgrades_total counter\n\
             sorobanpulse_protocol_downgrades_total {}\n",
            self.http3_requests.load(Ordering::Relaxed),
            self.http2_requests.load(Ordering::Relaxed),
            self.http1_requests.load(Ordering::Relaxed),
            self.protocol_downgrades.load(Ordering::Relaxed),
        )
    }
}

/// A single latency sample from a performance comparison run, used by the
/// benchmark harness to compare HTTP/3 against HTTP/2.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProtocolBenchmarkSample {
    pub protocol: String,
    pub latency_ms: f64,
    pub payload_bytes: u64,
}

/// Summarizes a set of benchmark samples per protocol (mean latency, throughput).
pub fn summarize_benchmark(samples: &[ProtocolBenchmarkSample]) -> HashMap<String, (f64, f64)> {
    let mut grouped: HashMap<String, Vec<&ProtocolBenchmarkSample>> = HashMap::new();
    for sample in samples {
        grouped.entry(sample.protocol.clone()).or_default().push(sample);
    }

    grouped
        .into_iter()
        .map(|(protocol, items)| {
            let count = items.len() as f64;
            let avg_latency = items.iter().map(|s| s.latency_ms).sum::<f64>() / count;
            let total_bytes: u64 = items.iter().map(|s| s.payload_bytes).sum();
            let throughput_kbps = (total_bytes as f64 / 1024.0)
                / (items.iter().map(|s| s.latency_ms).sum::<f64>() / 1000.0).max(0.001);
            (protocol, (avg_latency, throughput_kbps))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn negotiates_http3_when_offered_and_enabled() {
        let result = negotiate_protocol(&["h2".into(), "h3".into()], true);
        assert_eq!(result.selected, Protocol::Http3);
        assert!(!result.fell_back);
    }

    #[test]
    fn falls_back_to_http2_when_h3_disabled() {
        let result = negotiate_protocol(&["h3".into(), "h2".into()], false);
        assert_eq!(result.selected, Protocol::Http2);
        assert!(result.fell_back);
    }

    #[test]
    fn falls_back_to_http1_when_nothing_else_offered() {
        let result = negotiate_protocol(&["http/1.1".into()], true);
        assert_eq!(result.selected, Protocol::Http1_1);
        assert!(result.fell_back);
    }

    #[test]
    fn alt_svc_header_includes_port_and_max_age() {
        let cfg = Http3Config::default();
        let header = cfg.alt_svc_header();
        assert!(header.contains("h3=\":443\""));
        assert!(header.contains("ma=86400"));
    }

    #[test]
    fn connection_persistence_tracks_reuse() {
        let mut tracker = ConnectionPersistenceTracker::new();
        tracker.record_connection("client-1");
        tracker.record_connection("client-1");
        assert!(tracker.is_persisted("client-1"));
        assert_eq!(tracker.active_connections(), 1);
    }

    #[test]
    fn metrics_record_negotiation_outcomes() {
        let metrics = Http3Metrics::shared();
        metrics.record_negotiation(&negotiate_protocol(&["h3".into()], true));
        metrics.record_negotiation(&negotiate_protocol(&["http/1.1".into()], true));
        assert_eq!(metrics.http3_requests.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.http1_requests.load(Ordering::Relaxed), 1);
        assert_eq!(metrics.protocol_downgrades.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn benchmark_summary_computes_average_latency() {
        let samples = vec![
            ProtocolBenchmarkSample {
                protocol: "h3".into(),
                latency_ms: 10.0,
                payload_bytes: 1024,
            },
            ProtocolBenchmarkSample {
                protocol: "h3".into(),
                latency_ms: 20.0,
                payload_bytes: 1024,
            },
        ];
        let summary = summarize_benchmark(&samples);
        let (avg, _throughput) = summary.get("h3").unwrap();
        assert_eq!(*avg, 15.0);
    }
}
