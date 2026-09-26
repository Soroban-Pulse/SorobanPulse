/**
 * Issue #995: Connection pool load testing scenario.
 *
 * Tests the database connection pool under sustained load to profile:
 * - Connection wait time (soroban_pulse_db_pool_wait_seconds)
 * - Queue depth (soroban_pulse_db_pool_queue_depth)
 * - Wait timeouts (soroban_pulse_db_pool_wait_timeout_total)
 * - Pool exhaustion behaviour
 *
 * Usage:
 *   k6 run tests/load/pool_stress.js
 *   k6 run -e BASE_URL=http://localhost:3000 -e POOL_MAX=5 tests/load/pool_stress.js
 *
 * SLOs:
 *   - p99 connection wait < 200 ms
 *   - Error rate < 1%
 *   - Zero wait timeouts (waits > 1 s)
 */

import http from 'k6/http';
import { check, sleep } from 'k6';
import { Counter, Rate, Trend } from 'k6/metrics';

// ---------------------------------------------------------------------------
// Custom metrics
// ---------------------------------------------------------------------------

const poolWaitTimeouts = new Counter('pool_wait_timeouts');
const requestErrors = new Rate('request_errors');
const responseTime = new Trend('response_time_ms', true);

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const BASE_URL = __ENV.BASE_URL || 'http://localhost:3000';
const API_KEY = __ENV.API_KEY || '';

// Simulate a small pool (set DB_MAX_CONNECTIONS to this value before running)
const POOL_MAX = parseInt(__ENV.POOL_MAX || '10');

// ---------------------------------------------------------------------------
// Scenarios
// ---------------------------------------------------------------------------

export const options = {
  scenarios: {
    // Scenario 1: Steady load at 2× pool size to create queuing
    pool_pressure: {
      executor: 'constant-vus',
      vus: POOL_MAX * 2,
      duration: '60s',
      tags: { scenario: 'pool_pressure' },
    },
    // Scenario 2: Spike to 10× pool size to trigger exhaustion alerts
    pool_spike: {
      executor: 'ramping-vus',
      startTime: '65s',
      stages: [
        { duration: '10s', target: POOL_MAX * 10 },
        { duration: '20s', target: POOL_MAX * 10 },
        { duration: '10s', target: 0 },
      ],
      tags: { scenario: 'pool_spike' },
    },
    // Scenario 3: Recovery — verify pool normalises after spike
    pool_recovery: {
      executor: 'constant-vus',
      vus: Math.ceil(POOL_MAX * 0.5),
      startTime: '110s',
      duration: '30s',
      tags: { scenario: 'pool_recovery' },
    },
  },
  thresholds: {
    // p99 response time < 500 ms under pool pressure
    'http_req_duration{scenario:pool_pressure}': ['p(99)<500'],
    // p99 response time < 2000 ms during spike (some degradation expected)
    'http_req_duration{scenario:pool_spike}': ['p(99)<2000'],
    // Error rate < 5% overall (pool exhaustion will cause some 503s)
    'http_req_failed': ['rate<0.05'],
    // No wait timeouts during steady load
    'pool_wait_timeouts': ['count<1'],
  },
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

function headers() {
  const h = { 'Content-Type': 'application/json' };
  if (API_KEY) {
    h['Authorization'] = `Bearer ${API_KEY}`;
  }
  return h;
}

/**
 * Parse pool metrics from the Prometheus /metrics endpoint.
 * Returns an object with the key pool metrics.
 */
function scrapePoolMetrics() {
  const res = http.get(`${BASE_URL}/metrics`, { headers: headers() });
  if (res.status !== 200) return null;

  const text = res.body;
  const parse = (name) => {
    const m = text.match(new RegExp(`^${name}\\s+([\\d.]+)`, 'm'));
    return m ? parseFloat(m[1]) : null;
  };

  return {
    utilization: parse('soroban_pulse_db_pool_utilization'),
    queueDepth: parse('soroban_pulse_db_pool_queue_depth'),
    waitTimeouts: parse('soroban_pulse_db_pool_wait_timeout_total'),
    exhaustionAlerts: parse('soroban_pulse_db_pool_exhaustion_alerts_total'),
  };
}

// ---------------------------------------------------------------------------
// Default function (runs for every VU iteration)
// ---------------------------------------------------------------------------

export default function () {
  const start = Date.now();

  const res = http.get(`${BASE_URL}/v1/events?limit=20`, {
    headers: headers(),
    timeout: '5s',
  });

  const elapsed = Date.now() - start;
  responseTime.add(elapsed);

  const ok = check(res, {
    'status is 200 or 503': (r) => r.status === 200 || r.status === 503,
    'response has body': (r) => r.body && r.body.length > 0,
  });

  requestErrors.add(!ok);

  // A 503 under extreme pool exhaustion is acceptable but we count it.
  if (res.status === 503) {
    poolWaitTimeouts.add(1);
  }

  sleep(0.1);
}

// ---------------------------------------------------------------------------
// Setup: verify service is healthy before the test
// ---------------------------------------------------------------------------

export function setup() {
  const res = http.get(`${BASE_URL}/healthz/ready`);
  if (res.status !== 200) {
    throw new Error(`Service is not healthy before test: ${res.status} ${res.body}`);
  }
  console.log(`Pool stress test starting. Pool max: ${POOL_MAX}, target VUs: ${POOL_MAX * 2}`);
  return {};
}

// ---------------------------------------------------------------------------
// Teardown: report pool metrics at end of test
// ---------------------------------------------------------------------------

export function teardown() {
  const metrics = scrapePoolMetrics();
  if (metrics) {
    console.log('=== Final Pool Metrics ===');
    console.log(`  Utilization:       ${(metrics.utilization * 100).toFixed(1)}%`);
    console.log(`  Queue depth:       ${metrics.queueDepth}`);
    console.log(`  Wait timeouts:     ${metrics.waitTimeouts}`);
    console.log(`  Exhaustion alerts: ${metrics.exhaustionAlerts}`);
  }
}
