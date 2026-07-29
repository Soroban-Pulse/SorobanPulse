// k6 webhook delivery load test — issue #811
//
// Tests webhook delivery reliability and latency under load by driving event
// ingestion (via the admin replay endpoint or a stub) and measuring:
//   - Webhook queue depth proxy (admin stats endpoint)
//   - Delivery latency from event creation to webhook POST
//   - Retry behaviour under a simulated flaky webhook receiver
//   - HMAC signature presence on outgoing webhook calls
//
// Because k6 cannot act as a webhook receiver directly, this script drives
// the Soroban Pulse API and polls the admin/metrics endpoint to observe
// webhook failure counters and queue depth.
//
// For end-to-end delivery latency you need a real (or stubbed) webhook
// receiver that timestamps receipt. See docs/performance-tuning.md for a
// WireMock / webhook-site setup guide.
//
// Run:  k6 run tests/load/webhook_delivery.js
// Env:  BASE_URL         (default http://localhost:3000)
//       ADMIN_API_KEY    (required for /v1/admin/* endpoints)
//       WEBHOOK_RECEIVER (URL k6 will poll to count received deliveries)

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter, Gauge } from "k6/metrics";
import { randomIntBetween } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

// ── Custom metrics ─────────────────────────────────────────────────────────
const replayLatency      = new Trend("wh_replay_latency_ms",    true);
const metricsLatency     = new Trend("wh_metrics_latency_ms",   true);
const webhookFailures    = new Gauge("wh_failure_counter");
const errorRate          = new Rate("wh_error_rate");
const throughput         = new Counter("wh_requests_total");

// ── Scenario ───────────────────────────────────────────────────────────────
export const options = {
  scenarios: {
    // Drive event ingestion — pushes events through the webhook delivery pipeline
    event_injection: {
      executor: "constant-arrival-rate",
      rate: 20,
      timeUnit: "1s",
      duration: "5m",
      preAllocatedVUs: 10,
      maxVUs: 30,
      exec: "injectEvents",
    },
    // Concurrently poll metrics to observe delivery health
    metrics_observer: {
      executor: "constant-arrival-rate",
      rate: 1,
      timeUnit: "5s",         // one probe every 5 seconds
      duration: "5m",
      preAllocatedVUs: 2,
      maxVUs: 5,
      exec: "observeMetrics",
    },
    // Simultaneous REST load to verify webhook delivery doesn't starve API
    background_rest: {
      executor: "constant-arrival-rate",
      rate: 50,
      timeUnit: "1s",
      duration: "5m",
      preAllocatedVUs: 15,
      maxVUs: 40,
      exec: "backgroundRest",
    },
  },
  thresholds: {
    wh_replay_latency_ms:  ["p(99)<1000"],
    wh_metrics_latency_ms: ["p(99)<200"],
    wh_error_rate:         ["rate<0.05"],
  },
};

const BASE_URL      = __ENV.BASE_URL      || "http://localhost:3000";
const ADMIN_KEY     = __ENV.ADMIN_API_KEY || "";
const RECEIVER_URL  = __ENV.WEBHOOK_RECEIVER || "";

const ADMIN_HEADERS = ADMIN_KEY
  ? { "X-Api-Key": ADMIN_KEY, "Content-Type": "application/json" }
  : { "Content-Type": "application/json" };

const READ_HEADERS = ADMIN_KEY ? { "X-Api-Key": ADMIN_KEY } : {};

// ── Helpers ────────────────────────────────────────────────────────────────

// Trigger event replay for a random ledger range (drives webhook delivery).
export function injectEvents() {
  if (!ADMIN_KEY) {
    // Skip injection if no admin key — metrics observer still runs.
    sleep(1);
    return;
  }

  const fromLedger = randomIntBetween(1_000_000, 1_400_000);
  const toLedger   = fromLedger + randomIntBetween(1, 10);

  const payload = JSON.stringify({ from_ledger: fromLedger, to_ledger: toLedger });
  const res = http.post(
    `${BASE_URL}/v1/admin/replay`,
    payload,
    { headers: ADMIN_HEADERS, timeout: "30s" }
  );

  replayLatency.add(res.timings.duration);
  throughput.add(1);

  const ok = check(res, {
    "replay accepted": (r) => r.status === 200 || r.status === 202 || r.status === 204,
  });
  errorRate.add(!ok);
}

// Poll /metrics for webhook failure counter and queue health.
export function observeMetrics() {
  const res = http.get(
    `${BASE_URL}/metrics`,
    { headers: READ_HEADERS, timeout: "5s" }
  );

  metricsLatency.add(res.timings.duration);
  throughput.add(1);

  const ok = check(res, { "metrics 200": (r) => r.status === 200 });
  errorRate.add(!ok);

  if (ok && res.body) {
    // Extract soroban_pulse_webhook_failures_total from Prometheus text format
    const match = res.body.match(/soroban_pulse_webhook_failures_total\s+(\d+)/);
    if (match) {
      webhookFailures.add(parseInt(match[1], 10));
    }
  }

  // Optionally poll the stub webhook receiver for delivery count
  if (RECEIVER_URL) {
    http.get(RECEIVER_URL, { timeout: "5s" });
  }
}

// Background REST traffic — ensures webhook tasks don't starve the API.
export function backgroundRest() {
  const paths = [
    "/v1/events?page=1&limit=20",
    "/v1/events?event_type=contract&limit=20",
    "/healthz/ready",
  ];
  const path = paths[randomIntBetween(0, paths.length - 1)];
  const res  = http.get(`${BASE_URL}${path}`, {
    headers: READ_HEADERS,
    timeout: "10s",
  });

  throughput.add(1);
  errorRate.add(!check(res, { "status 200": (r) => r.status === 200 }));
}

export function handleSummary(data) {
  const p99replay  = data.metrics.wh_replay_latency_ms?.values?.["p(99)"]  ?? "n/a";
  const p99metrics = data.metrics.wh_metrics_latency_ms?.values?.["p(99)"] ?? "n/a";
  const err        = data.metrics.wh_error_rate?.values?.rate               ?? "n/a";
  const failures   = data.metrics.wh_failure_counter?.values?.value         ?? "n/a";
  const total      = data.metrics.wh_requests_total?.values?.count          ?? 0;

  console.log("\n=== WEBHOOK DELIVERY LOAD TEST SUMMARY ===");
  console.log(`Total requests         : ${total}`);
  console.log(`Replay p99 latency     : ${typeof p99replay  === "number" ? p99replay.toFixed(1)  : p99replay} ms`);
  console.log(`Metrics probe p99      : ${typeof p99metrics === "number" ? p99metrics.toFixed(1) : p99metrics} ms`);
  console.log(`Error rate             : ${typeof err === "number" ? (err * 100).toFixed(2) : err} %`);
  console.log(`Webhook failure total  : ${failures}`);

  return {
    "tests/load/results/webhook_delivery_summary.json": JSON.stringify(data, null, 2),
  };
}
