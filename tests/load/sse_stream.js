// k6 load test for GET /v1/events/stream (SSE endpoint)
// Tests long-lived connections, connection churn, and event delivery latency
//
// Run: k6 run tests/load/sse_stream.js
// Override base URL: k6 run -e BASE_URL=http://localhost:3000 tests/load/sse_stream.js

import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate, Counter } from "k6/metrics";
import { randomIntBetween } from "https://jslib.k6.io/k6-utils/1.4.0/index.js";

const connectionTime = new Trend("sse_connection_time", true);
const firstByteTime = new Trend("sse_first_byte_time", true);
const connectionErrors = new Rate("sse_connection_errors");
const eventsReceived = new Counter("sse_events_received");
const connectionChurnErrors = new Rate("sse_churn_errors");

export const options = {
  scenarios: {
    sustained_connections: {
      executor: "ramping-vus",
      startVUs: 0,
      stages: [
        { duration: "10s", target: 50 },  // Ramp up to 50 concurrent connections
        { duration: "30s", target: 50 },  // Hold for 30 seconds
        { duration: "10s", target: 0 },   // Ramp down
      ],
    },
    connection_churn: {
      executor: "ramping-arrival-rate",
      startRate: 0,
      timeUnit: "1s",
      stages: [
        { duration: "5s", target: 10 },   // Ramp up to 10 connects/sec
        { duration: "20s", target: 10 },  // Hold for 20 seconds
        { duration: "5s", target: 0 },    // Ramp down
      ],
      preAllocatedVUs: 20,
      maxVUs: 100,
    },
  },
  thresholds: {
    sse_connection_time: ["p(99)<500"],  // p99 connection establishment < 500ms
    sse_first_byte_time: ["p(99)<1000"], // p99 time to first byte < 1s
    sse_connection_errors: ["rate<0.05"],
    sse_churn_errors: ["rate<0.05"],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";

// Scenario 1: Sustained connections - hold connections open and verify content-type
export function sustained_connections() {
  const startTime = new Date();
  
  const res = http.get(`${BASE_URL}/v1/events/stream`, {
    headers: {
      "Accept": "text/event-stream",
    },
    timeout: "60s",
  });

  const connectionTimeMs = new Date() - startTime;
  connectionTime.add(connectionTimeMs);

  const ok = check(res, {
    "status 200": (r) => r.status === 200,
    "content-type is text/event-stream": (r) => 
      r.headers["Content-Type"] && r.headers["Content-Type"].includes("text/event-stream"),
    "connection established": (r) => r.status === 200,
  });

  if (!ok) {
    connectionErrors.add(1);
  }

  // Count events received (simple heuristic: count "data:" lines)
  if (res.body) {
    const eventCount = (res.body.match(/^data:/gm) || []).length;
    eventsReceived.add(eventCount);
  }

  // Hold connection open for a bit
  sleep(randomIntBetween(5, 15));
}

// Scenario 2: Connection churn - rapid connect/disconnect cycles
export function connection_churn() {
  const startTime = new Date();
  
  const res = http.get(`${BASE_URL}/v1/events/stream?contract_id=CABC123`, {
    headers: {
      "Accept": "text/event-stream",
    },
    timeout: "5s",
  });

  const connectionTimeMs = new Date() - startTime;
  firstByteTime.add(connectionTimeMs);

  const ok = check(res, {
    "status 200": (r) => r.status === 200,
    "content-type is text/event-stream": (r) => 
      r.headers["Content-Type"] && r.headers["Content-Type"].includes("text/event-stream"),
  });

  if (!ok) {
    connectionChurnErrors.add(1);
  }

  // Short hold before disconnecting
  sleep(randomIntBetween(0.5, 2));
}
