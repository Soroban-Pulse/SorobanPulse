# HTTP/3 Support

SorobanPulse supports HTTP/3 over QUIC for clients that negotiate it, with
automatic fallback to HTTP/2 and HTTP/1.1. This document describes
configuration, protocol negotiation, connection persistence, and the metrics
exposed for observability.

## Configuration

HTTP/3 is configured via `Http3Config` (`src/http3_support.rs`):

```rust
use sorobanpulse::http3_support::Http3Config;

let config = Http3Config {
    enabled: true,
    udp_bind_addr: "0.0.0.0".into(),
    udp_port: 443,
    alt_svc_max_age_secs: 86400,
    max_idle_timeout_ms: 30_000,
    max_concurrent_streams: 100,
    allow_0rtt: true,
    cert_path: "certs/fullchain.pem".into(),
    key_path: "certs/privkey.pem".into(),
};
```

| Field | Description |
|---|---|
| `enabled` | Master switch for the QUIC listener. |
| `udp_bind_addr` / `udp_port` | Address/port the QUIC (UDP) listener binds to. |
| `alt_svc_max_age_secs` | How long clients should cache the `Alt-Svc` advertisement. |
| `max_idle_timeout_ms` | Idle timeout before a QUIC connection is torn down. |
| `max_concurrent_streams` | Cap on concurrent bidirectional streams per connection. |
| `allow_0rtt` | Whether 0-RTT session resumption is permitted (trade-off: replay risk vs. latency). |

## QUIC Protocol Support

The QUIC transport terminates TLS 1.3 and multiplexes streams without
head-of-line blocking at the TCP layer, which is the main latency win over
HTTP/2 on lossy networks. The server advertises support via the `Alt-Svc`
response header:

```
Alt-Svc: h3=":443"; ma=86400
```

produced by `Http3Config::alt_svc_header()`.

## Protocol Negotiation

`negotiate_protocol(client_alpn, http3_enabled)` selects the best protocol a
client advertised via ALPN, preferring HTTP/3, then HTTP/2, then HTTP/1.1.
When HTTP/3 is disabled or not offered by the client, the result is marked
`fell_back = true` so callers can track downgrade rates.

## Connection Persistence

`ConnectionPersistenceTracker` keeps a lightweight in-memory record of which
clients have an established QUIC connection, so repeat requests can reuse it
instead of re-handshaking. Idle connections are evicted via `evict_idle`.

## Metrics

`Http3Metrics` counts requests per protocol, downgrade events, and 0-RTT
accept/reject outcomes, and exposes them in Prometheus exposition format via
`to_prometheus()`:

- `sorobanpulse_http3_requests_total`
- `sorobanpulse_http2_requests_total`
- `sorobanpulse_http1_requests_total`
- `sorobanpulse_protocol_downgrades_total`

## Performance Comparison

`ProtocolBenchmarkSample` and `summarize_benchmark` support recording latency
samples per protocol during load tests and summarizing average latency and
throughput, so HTTP/3 vs HTTP/2 performance can be compared directly. See
the unit tests in `src/http3_support.rs` for example usage.
