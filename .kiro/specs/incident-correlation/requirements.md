# Requirements Document

## Introduction

This feature adds an incident correlation engine to the Soroban Pulse backend service. When anomalous conditions are detected — such as elevated RPC error rates, indexer lag spikes, database pool exhaustion, or HTTP error surges — the system automatically links the relevant Prometheus metrics, structured log entries, and (when available) OpenTelemetry traces into a unified incident record. It then builds a chronological timeline of related signals, detects co-occurring events across signal types, and surfaces root cause analysis (RCA) suggestions to operators. The feature is implemented entirely in Rust within the existing Axum/Tokio/SQLx/PostgreSQL stack and exposes its findings through new REST API endpoints.

## Glossary

- **Correlation_Engine**: The Soroban Pulse subsystem responsible for detecting incidents, aggregating correlated signals, building timelines, and generating RCA suggestions.
- **Incident**: A named anomalous condition detected from one or more signals (metric threshold breach, log error burst, or trace error span), persisted in PostgreSQL with a unique UUID.
- **Signal**: A discrete observable datum — either a Metric_Sample, a Log_Entry, or a Trace_Span — associated with a timestamp and optional label set.
- **Metric_Sample**: A named Prometheus counter or gauge value captured at a specific timestamp (e.g., `soroban_pulse_indexer_lag_ledgers`, `soroban_pulse_rpc_errors_total`).
- **Log_Entry**: A structured JSON log record emitted by `tracing-subscriber`, containing at minimum a `timestamp`, `level`, `target`, and `message` field.
- **Trace_Span**: An OpenTelemetry span record available when the `otel` feature flag is enabled, identified by a `trace_id` and `span_id`.
- **Correlation_Window**: A configurable time interval (default 5 minutes) used to group signals that occur within proximity of a detected anomaly.
- **Timeline**: An ordered sequence of correlated Signal records spanning the Correlation_Window around an Incident, sorted ascending by timestamp.
- **RCA_Suggestion**: A human-readable string produced by a deterministic rule engine that maps observed signal patterns to probable root causes.
- **Detector**: A component of the Correlation_Engine that continuously evaluates threshold rules against current Metric_Sample values and emits Incident records when rules fire.
- **Signal_Store**: The PostgreSQL tables (`incidents`, `incident_signals`) that persist Incident and Signal records.
- **API_Server**: The existing Axum HTTP server that hosts all Soroban Pulse REST endpoints.

## Requirements

### Requirement 1: Incident Detection from Metric Thresholds

**User Story:** As an operator, I want the system to automatically open an incident when a known metric threshold is breached, so that I am notified without manual Prometheus query work.

#### Acceptance Criteria

1. WHEN `soroban_pulse_indexer_lag_ledgers` exceeds 100, THE Detector SHALL create an Incident with kind `indexer_lag_high` and severity `warning`.
2. WHEN `soroban_pulse_indexer_lag_ledgers` exceeds 500, THE Detector SHALL create an Incident with kind `indexer_lag_critical` and severity `critical`.
3. WHEN the rate of `soroban_pulse_rpc_errors_total` over a 5-minute window exceeds 5% of total RPC calls, THE Detector SHALL create an Incident with kind `rpc_error_rate_high` and severity `critical`.
4. WHEN `soroban_pulse_db_pool_size` equals `soroban_pulse_db_pool_max` for 60 consecutive seconds, THE Detector SHALL create an Incident with kind `db_pool_exhausted` and severity `critical`.
5. WHEN the HTTP 5xx error rate over a 5-minute window exceeds 1% of total HTTP requests, THE Detector SHALL create an Incident with kind `http_error_rate_high` and severity `critical`.
6. WHEN no successful indexer poll has occurred for 120 seconds, THE Detector SHALL create an Incident with kind `indexer_stall` and severity `critical`.
7. THE Detector SHALL evaluate all threshold rules at an interval no greater than 30 seconds.
8. WHILE an Incident of a given kind is already in `open` status, THE Detector SHALL NOT create a duplicate Incident of the same kind.
9. IF a metric value returns within normal range after an Incident is created, THEN THE Detector SHALL update the Incident status to `resolved` and record a `resolved_at` timestamp.

---

### Requirement 2: Signal Collection and Association

**User Story:** As an operator, I want correlated metrics, logs, and traces to be linked to an incident automatically, so that I have all relevant evidence in one place.

#### Acceptance Criteria

1. WHEN an Incident is created, THE Correlation_Engine SHALL collect all Metric_Sample values from the preceding Correlation_Window and associate them with the Incident.
2. WHEN an Incident is created, THE Correlation_Engine SHALL scan the structured log buffer for Log_Entry records within the Correlation_Window that have `level` of `WARN` or `ERROR` and associate them with the Incident.
3. WHERE the `otel` feature flag is enabled, WHEN an Incident is created, THE Correlation_Engine SHALL associate Trace_Span records within the Correlation_Window that have a non-zero error status with the Incident.
4. THE Correlation_Engine SHALL store each associated Signal in the Signal_Store with the fields: `incident_id`, `signal_type` (one of `metric`, `log`, `trace`), `source_name`, `observed_at`, and `payload` (JSON).
5. THE Correlation_Engine SHALL collect signals from at least two distinct signal types (e.g., both `metric` and `log`) when both are available during the Correlation_Window.
6. IF no signals are found within the Correlation_Window, THEN THE Correlation_Engine SHALL still persist the Incident with an empty signal list and annotate it with `signals_empty: true`.

---

### Requirement 3: Incident Timeline Construction

**User Story:** As an operator, I want to view a chronological timeline of all signals associated with an incident, so that I can understand the sequence of events that led to the problem.

#### Acceptance Criteria

1. THE Correlation_Engine SHALL build a Timeline for each Incident by sorting all associated Signals in ascending order by `observed_at` timestamp.
2. WHEN two Signals share an identical `observed_at` timestamp, THE Correlation_Engine SHALL order them by `signal_type` in the sequence `metric` → `log` → `trace`.
3. THE API_Server SHALL expose a `GET /v1/incidents/{incident_id}/timeline` endpoint that returns the Timeline as a JSON array of Signal records.
4. WHEN a valid `incident_id` is provided, THE API_Server SHALL return the Timeline with HTTP status 200.
5. IF the `incident_id` does not exist in the Signal_Store, THEN THE API_Server SHALL return HTTP status 404 with an `ErrorResponse` body using error code `incident_not_found`.
6. THE Timeline returned by `GET /v1/incidents/{incident_id}/timeline` SHALL include a top-level `incident` object alongside the `signals` array, containing at minimum: `id`, `kind`, `severity`, `status`, `opened_at`, and `resolved_at`.

---

### Requirement 4: Related Event Detection

**User Story:** As an operator, I want the system to identify other incidents or signals that are temporally or causally related to the current incident, so that I can understand blast radius and cascading failures.

#### Acceptance Criteria

1. THE Correlation_Engine SHALL identify Related_Incidents as any other Incidents whose `opened_at` falls within twice the Correlation_Window of the current Incident's `opened_at`.
2. WHEN two Incidents share at least one Signal source name (e.g., the same metric name or log target), THE Correlation_Engine SHALL mark them as `correlated` with a `correlation_reason` of `shared_signal_source`.
3. THE API_Server SHALL expose a `GET /v1/incidents/{incident_id}/related` endpoint that returns Related_Incidents as a JSON array, each with `id`, `kind`, `severity`, `status`, `opened_at`, and `correlation_reason`.
4. WHEN no related incidents exist, THE API_Server SHALL return an empty JSON array with HTTP status 200.
5. THE Correlation_Engine SHALL limit the Related_Incidents result set to a maximum of 20 entries, ordered by ascending temporal distance from the current Incident.

---

### Requirement 5: Root Cause Analysis Suggestions

**User Story:** As an operator, I want the system to suggest probable root causes based on observed signal patterns, so that I can begin remediation faster.

#### Acceptance Criteria

1. THE Correlation_Engine SHALL apply RCA rules deterministically: for a given set of correlated signal kinds, the same RCA_Suggestion SHALL always be produced.
2. WHEN an Incident of kind `indexer_lag_high` or `indexer_lag_critical` is correlated with Log_Entry records containing the substring `RPC error`, THE Correlation_Engine SHALL produce an RCA_Suggestion of `Probable cause: RPC endpoint degradation. Check RPC connectivity and consider switching to a backup node.`
3. WHEN an Incident of kind `db_pool_exhausted` is correlated with Log_Entry records containing the substring `pool timed out`, THE Correlation_Engine SHALL produce an RCA_Suggestion of `Probable cause: Database connection pool saturation. Increase DB_MAX_CONNECTIONS or reduce query concurrency.`
4. WHEN an Incident of kind `http_error_rate_high` is correlated with an Incident of kind `db_pool_exhausted` within the same Correlation_Window, THE Correlation_Engine SHALL produce an RCA_Suggestion of `Probable cause: HTTP errors driven by database unavailability. Resolve database pool exhaustion first.`
5. WHEN an Incident of kind `indexer_stall` is present and no Log_Entry records with `ERROR` level are found in the Correlation_Window, THE Correlation_Engine SHALL produce an RCA_Suggestion of `Probable cause: Silent indexer stall, possibly advisory lock contention. Check replica count and advisory lock holder.`
6. WHEN no RCA rule matches the observed signal pattern, THE Correlation_Engine SHALL produce an RCA_Suggestion of `No specific root cause identified. Review correlated signals in the timeline for manual analysis.`
7. THE API_Server SHALL expose a `GET /v1/incidents/{incident_id}/rca` endpoint that returns the RCA_Suggestion as a JSON object with fields `incident_id`, `suggestion`, and `matched_rule` (the rule identifier that fired, or `none`).
8. THE Correlation_Engine SHALL evaluate RCA rules in priority order and return only the highest-priority matching suggestion per incident.

---

### Requirement 6: Incident Listing and Filtering

**User Story:** As an operator, I want to list and filter incidents, so that I can monitor system health and review historical incidents.

#### Acceptance Criteria

1. THE API_Server SHALL expose a `GET /v1/incidents` endpoint that returns a paginated list of Incidents ordered by `opened_at` descending.
2. WHEN a `status` query parameter of `open` or `resolved` is provided, THE API_Server SHALL filter the result to Incidents matching that status.
3. WHEN a `severity` query parameter of `warning` or `critical` is provided, THE API_Server SHALL filter the result to Incidents matching that severity.
4. WHEN a `from` and `to` query parameter are provided as ISO 8601 timestamps, THE API_Server SHALL filter Incidents whose `opened_at` falls within the inclusive range `[from, to]`.
5. THE API_Server SHALL support `page` and `limit` query parameters with `limit` clamped to the range [1, 100] and defaulting to 20.
6. IF an invalid `status` or `severity` value is provided, THEN THE API_Server SHALL return HTTP status 400 with an `ErrorResponse` body using error code `invalid_filter_param`.

---

### Requirement 7: Signal_Store Schema and Data Persistence

**User Story:** As a developer, I want incidents and signals stored in PostgreSQL with appropriate indices, so that queries remain fast as data grows.

#### Acceptance Criteria

1. THE Signal_Store SHALL persist Incidents in a table named `incidents` with columns: `id UUID PRIMARY KEY`, `kind TEXT NOT NULL`, `severity TEXT NOT NULL`, `status TEXT NOT NULL DEFAULT 'open'`, `opened_at TIMESTAMPTZ NOT NULL`, `resolved_at TIMESTAMPTZ`, `signals_empty BOOLEAN NOT NULL DEFAULT false`, `metadata JSONB`.
2. THE Signal_Store SHALL persist Signals in a table named `incident_signals` with columns: `id UUID PRIMARY KEY`, `incident_id UUID NOT NULL REFERENCES incidents(id)`, `signal_type TEXT NOT NULL`, `source_name TEXT NOT NULL`, `observed_at TIMESTAMPTZ NOT NULL`, `payload JSONB NOT NULL`.
3. THE Signal_Store SHALL create an index on `incidents(status, opened_at DESC)` to support filtered listing queries.
4. THE Signal_Store SHALL create an index on `incident_signals(incident_id, observed_at ASC)` to support timeline queries.
5. THE Signal_Store SHALL create an index on `incidents(kind, opened_at DESC)` to support deduplication checks by the Detector.
6. THE Correlation_Engine SHALL use SQLx prepared statements for all Signal_Store reads and writes.
7. FOR ALL Incident records written then read back from the Signal_Store, THE Signal_Store SHALL return an equivalent record (round-trip property: write then read produces the same `id`, `kind`, `severity`, `status`, `opened_at`).

---

### Requirement 8: Configuration

**User Story:** As an operator, I want to configure correlation window size and detection thresholds via environment variables, so that I can tune the system without recompiling.

#### Acceptance Criteria

1. THE Correlation_Engine SHALL read the Correlation_Window duration from the environment variable `CORRELATION_WINDOW_SECS`, defaulting to 300 (5 minutes) when the variable is absent.
2. THE Correlation_Engine SHALL read the detector evaluation interval from `INCIDENT_DETECTOR_INTERVAL_SECS`, defaulting to 30 when the variable is absent.
3. IF `CORRELATION_WINDOW_SECS` is set to a value less than 60 or greater than 3600, THEN THE Correlation_Engine SHALL log a warning and clamp the value to the range [60, 3600].
4. IF `INCIDENT_DETECTOR_INTERVAL_SECS` is set to a value less than 10 or greater than 300, THEN THE Correlation_Engine SHALL log a warning and clamp the value to the range [10, 300].
5. THE Correlation_Engine SHALL expose the active configuration values (post-clamping) in the structured startup log at `INFO` level.

---

### Requirement 9: Observability of the Correlation Engine Itself

**User Story:** As an operator, I want to monitor the health of the correlation engine via Prometheus metrics, so that I know if incident detection is working.

#### Acceptance Criteria

1. THE Correlation_Engine SHALL emit a counter `soroban_pulse_incidents_created_total` incremented each time a new Incident is persisted, labelled by `kind` and `severity`.
2. THE Correlation_Engine SHALL emit a counter `soroban_pulse_incidents_resolved_total` incremented each time an Incident transitions to `resolved`, labelled by `kind`.
3. THE Correlation_Engine SHALL emit a gauge `soroban_pulse_incidents_open` reflecting the current count of Incidents with status `open`.
4. THE Correlation_Engine SHALL emit a histogram `soroban_pulse_correlation_duration_seconds` recording the wall-clock time taken to collect and persist signals for each Incident.
5. WHEN a detector evaluation cycle fails due to a database error, THE Correlation_Engine SHALL increment a counter `soroban_pulse_detector_errors_total` and log the error at `ERROR` level.
