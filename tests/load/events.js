// k6 load test for GET /v1/events
// Target SLO: p99 latency < 200ms at 100 req/s
//
// Run: k6 run tests/load/events.js
// Override base URL: k6 run -e BASE_URL=http://localhost:3000 tests/load/events.js

import http from "k6/http";
import { check } from "k6";
import { Trend, Rate } from "k6/metrics";

const latency = new Trend("events_latency", true);
const errorRate = new Rate("events_errors");

export const options = {
  scenarios: {
    steady_load: {
      executor: "constant-arrival-rate",
      rate: 100,
      timeUnit: "1s",
      duration: "30s",
      preAllocatedVUs: 20,
      maxVUs: 50,
    },
  },
  thresholds: {
    events_latency: ["p(99)<200"],
    events_errors: ["rate<0.01"],
  },
};

const BASE_URL = __ENV.BASE_URL || "http://localhost:3000";

export default function () {
  const res = http.get(`${BASE_URL}/v1/events?page=1&limit=20`);
  latency.add(res.timings.duration);
  const ok = check(res, { "status 200": (r) => r.status === 200 });
  errorRate.add(!ok);
}
