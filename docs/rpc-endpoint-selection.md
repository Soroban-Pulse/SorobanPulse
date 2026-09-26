# Latency-Scored RPC Endpoint Selection

Maintain a list of candidate RPC endpoints and score them by recent latency, error rate, and retention depth. Prefer the lowest-latency healthy endpoint and periodically re-probe degraded endpoints.
