# Requirements Document

## Introduction

The Derived Metrics Framework enables the Soroban Pulse system to compute and expose derived metrics from existing base metrics using a declarative Domain Specific Language (DSL). The framework supports metric transformations, aggregations, and compositions to provide higher-level observability insights such as throughput rates, error rates, utilization percentages, and statistical aggregations without requiring manual instrumentation changes.

## Glossary

- **Base_Metric**: A primitive metric collected directly from system instrumentation (counter, gauge, or histogram)
- **Derived_Metric**: A computed metric calculated from one or more Base_Metrics using transformation functions
- **Metric_DSL**: The Domain Specific Language used to define Derived_Metric transformations
- **Metric_Engine**: The runtime component that evaluates Metric_DSL definitions and computes Derived_Metrics
- **Aggregation_Function**: A mathematical operation applied to Base_Metrics (e.g., rate, delta, ratio, moving_average)
- **Metric_Registry**: The component that stores and retrieves Base_Metric values and metadata
- **Metric_Definition_Parser**: The component that parses Metric_DSL syntax into executable transformations
- **Prometheus_Exporter**: The component that exposes metrics at the /metrics endpoint in Prometheus format
- **Evaluation_Window**: The time period over which time-based aggregations are computed
- **Metric_Label**: A key-value pair that provides dimensional metadata for a metric

## Requirements

### Requirement 1: Metric DSL Definition

**User Story:** As a developer, I want to define derived metrics using a declarative DSL, so that I can express metric transformations without writing imperative code

#### Acceptance Criteria

1. THE Metric_DSL SHALL support rate() function syntax for computing per-second rates from counter metrics
2. THE Metric_DSL SHALL support delta() function syntax for computing differences between consecutive metric readings
3. THE Metric_DSL SHALL support ratio() function syntax for computing proportions between two metrics
4. THE Metric_DSL SHALL support moving_average() function syntax with configurable window duration
5. THE Metric_DSL SHALL support percentile() function syntax for histogram metrics with configurable percentile values
6. THE Metric_DSL SHALL support arithmetic operators (+, -, \*, /) for combining metric expressions
7. THE Metric_DSL SHALL support parentheses for grouping and precedence control
8. THE Metric_DSL SHALL support metric references using Base_Metric names from the Metric_Registry
9. THE Metric_DSL SHALL support numeric literal constants in expressions

### Requirement 2: Metric Definition Parser

**User Story:** As a system administrator, I want the system to parse metric definitions at startup, so that invalid definitions are caught before runtime

#### Acceptance Criteria

1. WHEN the Metric_Engine initializes, THE Metric_Definition_Parser SHALL parse all registered metric definitions
2. IF a metric definition contains invalid syntax, THEN THE Metric_Definition_Parser SHALL return a descriptive error message with line and column position
3. IF a metric definition references an undefined Base_Metric, THEN THE Metric_Definition_Parser SHALL return an error identifying the missing metric name
4. IF a metric definition contains a type mismatch, THEN THE Metric_Definition_Parser SHALL return an error identifying the incompatible types
5. THE Metric_Definition_Parser SHALL validate that aggregation function parameters are within supported ranges
6. THE Metric_Definition_Parser SHALL detect circular dependencies between Derived_Metrics
7. WHEN parsing completes successfully, THE Metric_Definition_Parser SHALL produce an Abstract Syntax Tree representation
8. FOR ALL valid metric definitions, parsing then serializing then parsing SHALL produce an equivalent Abstract Syntax Tree

### Requirement 3: Metric Evaluation Engine

**User Story:** As an operations engineer, I want derived metrics to be computed efficiently, so that metric collection does not impact system performance

#### Acceptance Criteria

1. WHEN the Prometheus_Exporter receives a scrape request, THE Metric_Engine SHALL evaluate all Derived_Metric definitions
2. THE Metric_Engine SHALL cache Base_Metric values for the duration of a single evaluation cycle
3. THE Metric_Engine SHALL compute Derived_Metrics in topological dependency order
4. THE Metric_Engine SHALL complete all metric evaluations within 100 milliseconds for up to 100 derived metrics
5. IF a Derived_Metric evaluation fails, THEN THE Metric_Engine SHALL log the error and skip that metric without affecting other metrics
6. THE Metric_Engine SHALL maintain time-series history for time-based aggregations with configurable retention duration
7. THE Metric_Engine SHALL support concurrent evaluation of independent Derived_Metrics

### Requirement 4: Rate Aggregation Function

**User Story:** As a monitoring engineer, I want to compute per-second rates from counter metrics, so that I can track throughput and event frequencies

#### Acceptance Criteria

1. WHEN rate() is applied to a counter metric, THE Metric_Engine SHALL compute the per-second rate of change
2. THE rate() function SHALL use a configurable Evaluation_Window between 10 seconds and 300 seconds
3. IF fewer than two data points exist within the Evaluation_Window, THEN THE Metric_Engine SHALL return zero
4. THE rate() function SHALL handle counter resets by detecting negative deltas and treating them as resets to zero
5. THE Metric_Engine SHALL compute rate() using linear regression over all data points in the Evaluation_Window

### Requirement 5: Delta Aggregation Function

**User Story:** As a developer, I want to compute the difference between consecutive metric readings, so that I can track incremental changes

#### Acceptance Criteria

1. WHEN delta() is applied to a gauge metric, THE Metric_Engine SHALL compute the difference from the previous reading
2. WHEN delta() is applied to a counter metric, THE Metric_Engine SHALL compute the absolute change since the previous reading
3. IF no previous reading exists, THEN THE Metric_Engine SHALL return zero
4. THE delta() function SHALL support an optional lookback duration parameter defaulting to the scrape interval

### Requirement 6: Ratio Aggregation Function

**User Story:** As an SRE, I want to compute ratios between metrics, so that I can calculate percentages and proportions

#### Acceptance Criteria

1. WHEN ratio() is applied to two metrics, THE Metric_Engine SHALL divide the numerator metric by the denominator metric
2. IF the denominator metric equals zero, THEN THE Metric_Engine SHALL return zero
3. THE ratio() function SHALL support an optional scale parameter for converting to percentages
4. THE ratio() function SHALL preserve Metric_Labels from both input metrics using a configurable join strategy

### Requirement 7: Moving Average Aggregation Function

**User Story:** As a monitoring engineer, I want to smooth noisy metrics using moving averages, so that I can identify trends

#### Acceptance Criteria

1. WHEN moving_average() is applied to a metric, THE Metric_Engine SHALL compute the arithmetic mean over the Evaluation_Window
2. THE moving_average() function SHALL require an Evaluation_Window parameter between 30 seconds and 3600 seconds
3. THE Metric_Engine SHALL maintain a sliding window of historical values with timestamp precision
4. IF fewer than 3 data points exist within the Evaluation_Window, THEN THE Metric_Engine SHALL return the most recent value

### Requirement 8: Percentile Aggregation Function

**User Story:** As an SRE, I want to compute percentiles from histogram metrics, so that I can track latency distributions

#### Acceptance Criteria

1. WHEN percentile() is applied to a histogram metric, THE Metric_Engine SHALL compute the specified percentile value
2. THE percentile() function SHALL accept percentile parameters between 0.0 and 100.0
3. THE Metric_Engine SHALL use linear interpolation for percentile estimation when exact bucket boundaries are not available
4. THE percentile() function SHALL support multiple percentile computations from the same histogram in a single evaluation

### Requirement 9: Metric Definition Configuration

**User Story:** As a system administrator, I want to define derived metrics in configuration files, so that I can manage them without code changes

#### Acceptance Criteria

1. THE Metric_Engine SHALL load metric definitions from a TOML configuration file at startup
2. THE configuration file SHALL support a [[derived_metrics]] array with name, expression, description, and labels fields
3. THE Metric_Engine SHALL support hot-reloading of metric definitions on SIGHUP signal
4. IF hot-reloading fails due to invalid definitions, THEN THE Metric_Engine SHALL retain the previous valid definitions and log an error
5. THE configuration format SHALL support multi-line expressions using TOML multi-line string syntax

### Requirement 10: Metric Export Integration

**User Story:** As an operations engineer, I want derived metrics exposed via the existing Prometheus endpoint, so that they integrate with existing monitoring infrastructure

#### Acceptance Criteria

1. WHEN the Prometheus_Exporter receives a scrape request, THE Metric_Engine SHALL include all Derived_Metrics in the response
2. THE Prometheus*Exporter SHALL prefix Derived_Metric names with "soroban_pulse_derived*"
3. THE Prometheus_Exporter SHALL include a "derived=true" label on all Derived_Metrics
4. THE Prometheus_Exporter SHALL include HELP and TYPE metadata for each Derived_Metric
5. THE Derived_Metric SHALL preserve Metric_Labels from Base_Metrics when applicable
6. THE Prometheus_Exporter SHALL expose a "soroban_pulse_derived_metrics_evaluation_duration_seconds" histogram tracking evaluation time

### Requirement 11: Common Derived Metric Examples

**User Story:** As a new user, I want pre-configured examples of useful derived metrics, so that I can quickly benefit from the framework

#### Acceptance Criteria

1. THE Metric_Engine SHALL provide an example configuration file with commented metric definitions
2. THE example configuration SHALL include indexer_throughput_events_per_second using rate() on events_indexed_total
3. THE example configuration SHALL include rpc_error_rate using ratio() of rpc_errors_total to total requests
4. THE example configuration SHALL include db_pool_utilization_percent using ratio() of active connections to max connections
5. THE example configuration SHALL include http_request_latency_p95 using percentile() on http_request_duration_seconds
6. THE example configuration SHALL include indexer_lag_smoothed using moving_average() on indexer_lag_ledgers

### Requirement 12: Error Handling and Diagnostics

**User Story:** As a developer, I want clear error messages when metric evaluation fails, so that I can debug issues quickly

#### Acceptance Criteria

1. IF a Base_Metric referenced in a definition is not found, THEN THE Metric_Engine SHALL log a warning with the metric name and definition
2. IF a metric evaluation throws an exception, THEN THE Metric_Engine SHALL log the stack trace and expression that failed
3. THE Metric_Engine SHALL expose a "soroban_pulse_derived_metrics_evaluation_errors_total" counter tracking evaluation failures
4. THE Metric_Engine SHALL expose a "soroban_pulse_derived_metrics_parse_errors_total" counter tracking parse failures
5. WHEN a Derived_Metric fails to evaluate, THE Metric_Engine SHALL omit it from the Prometheus response rather than returning stale data

### Requirement 13: Memory and Storage Management

**User Story:** As a system administrator, I want the framework to manage memory efficiently, so that long-running deployments remain stable

#### Acceptance Criteria

1. THE Metric_Engine SHALL enforce a maximum history retention duration configurable between 300 seconds and 7200 seconds
2. THE Metric_Engine SHALL evict historical data points older than the retention duration
3. THE Metric_Engine SHALL limit in-memory history to a maximum of 10000 data points per Base_Metric
4. IF the data point limit is exceeded, THEN THE Metric_Engine SHALL evict the oldest data points
5. THE Metric_Engine SHALL expose a "soroban_pulse_derived_metrics_history_size_bytes" gauge tracking memory usage

### Requirement 14: Label Composition and Filtering

**User Story:** As a monitoring engineer, I want to aggregate metrics across label dimensions, so that I can create summary metrics

#### Acceptance Criteria

1. THE Metric_DSL SHALL support label filtering syntax using brackets: metric_name{label="value"}
2. THE Metric_DSL SHALL support sum_over() function to aggregate metrics across all label values
3. THE Metric_DSL SHALL support group_by() function to preserve specific labels while aggregating others
4. IF a label filter matches no metrics, THEN THE Metric_Engine SHALL return zero
5. THE Metric_Engine SHALL support combining metrics with different label sets using outer join semantics

### Requirement 15: Testing and Validation Framework

**User Story:** As a developer, I want to write tests for metric definitions, so that I can verify correctness

#### Acceptance Criteria

1. THE Metric_Engine SHALL provide a test harness accepting mock Base_Metric values and expected Derived_Metric outputs
2. THE test harness SHALL support time-based test scenarios with simulated scrape intervals
3. THE test harness SHALL validate that evaluation results match expected values within a configurable tolerance
4. THE test harness SHALL report mismatches with actual vs expected values and the expression evaluated
5. THE framework SHALL include unit tests for all aggregation functions using property-based testing
