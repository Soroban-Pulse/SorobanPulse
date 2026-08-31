# AWS EventBridge Integration

Issue #954: Create integration with AWS EventBridge for event routing.

## Overview

The AWS EventBridge integration enables SorobanPulse to route Soroban events through AWS EventBridge for sophisticated event processing and routing. Key features include:

- **Event submission to EventBridge** with automatic batching
- **Event filtering** with custom patterns
- **EventBridge rule management** (create, update, delete, describe)
- **Cross-account EventBridge support** using IAM role assumption
- **Automatic retry logic** for resilient event delivery

## Architecture

Events flow through EventBridge as follows:

```
SorobanPulse Events
        ↓
   EventBridge
        ↓
   ┌─────────────────────────────────────┐
   │  Event Filtering & Pattern Matching │
   └─────────────────────────────────────┘
        ↓
   EventBridge Rules
        ↓
   ┌─────────────────────────┐
   │  Event Targets          │
   │  - Lambda               │
   │  - SNS                  │
   │  - SQS                  │
   │  - Kinesis Data Streams │
   │  - Custom HTTP Targets  │
   └─────────────────────────┘
```

## Configuration

### Environment Variables

```bash
# AWS region (required)
AWS_REGION=us-east-1

# EventBridge event bus name (optional, defaults to "default")
EVENTBRIDGE_EVENT_BUS=custom-bus

# Cross-account role ARN for cross-account EventBridge access (optional)
EVENTBRIDGE_CROSS_ACCOUNT_ROLE_ARN=arn:aws:iam::123456789012:role/EventBridgeRole

# AWS credentials (standard AWS credential chain)
AWS_ACCESS_KEY_ID=...
AWS_SECRET_ACCESS_KEY=...
AWS_SESSION_TOKEN=...
```

### Programmatic Configuration

```rust
use soroban_pulse::eventbridge::{
    EventBridgeConfig, AwsEventBridgePublisher, EventPattern
};
use std::collections::HashMap;

let config = EventBridgeConfig {
    event_bus_name: "soroban-events".to_string(),
    region: "us-east-1".to_string(),
    source: "soroban-pulse".to_string(),
    detail_type: "SorobanEvent".to_string(),
    event_pattern: Some(EventPattern {
        contract_id: Some(vec![
            "CABCDEF123...".to_string(),
            "CXYZDEF456...".to_string(),
        ]),
        event_type: Some(vec!["Transfer".to_string(), "Burn".to_string()]),
        detail_fields: None,
    }),
    batch_size: 10,
    timeout_secs: 10,
    max_retries: 3,
    cross_account_role_arn: None,
    rule_name: None,
};

let publisher = AwsEventBridgePublisher::new(config);
```

## Event Filtering

### Pattern Matching

Events can be filtered before sending to EventBridge:

```rust
use soroban_pulse::eventbridge::EventPattern;

let pattern = EventPattern {
    // Match specific contracts
    contract_id: Some(vec![
        "contract1".to_string(),
        "contract2".to_string(),
    ]),
    
    // Match specific event types
    event_type: Some(vec!["Transfer".to_string()]),
    
    // Additional detail field matching
    detail_fields: Some(HashMap::from([
        ("amount_greater_than".to_string(), vec!["1000000".to_string()]),
    ])),
};

assert!(pattern.matches(&soroban_event));
```

### EventBridge Rules

Define rules within EventBridge for complex event routing:

```rust
use soroban_pulse::eventbridge::{EventBridgeRule, RuleState};

let rule = EventBridgeRule {
    name: "high-value-transfers".to_string(),
    description: Some("Route high-value transfer events to analytics".to_string()),
    event_pattern: r#"{
        "source": ["soroban-pulse"],
        "detail-type": ["SorobanEvent"],
        "detail": {
            "event_type": ["Transfer"],
            "amount": [{"numeric": [">", 1000000]}]
        }
    }"#.to_string(),
    state: RuleState::ENABLED,
    event_bus_name: "soroban-events".to_string(),
};

publisher.put_rule(rule).await?;
```

## Usage Examples

### Submitting Events

```rust
use soroban_pulse::eventbridge::EventBridgePublisher;

let events = vec![soroban_event1, soroban_event2];

match publisher.put_events(events).await {
    Ok(response) => {
        println!("Successfully submitted {} events", response.entries.len());
        if response.failed_entry_count > 0 {
            eprintln!("Failed entries: {}", response.failed_entry_count);
        }
    }
    Err(e) => eprintln!("Error: {}", e),
}
```

### Managing Rules

```rust
// Create a rule
publisher.put_rule(rule).await?;

// Check if rule exists
if let Some(existing) = publisher.describe_rule("high-value-transfers").await? {
    println!("Rule found: {:?}", existing);
}

// Delete a rule
publisher.delete_rule("high-value-transfers").await?;
```

## Metrics

The integration tracks EventBridge operations via metrics:

- `soroban_pulse_eventbridge_put_events_success_total` - Counter of successfully submitted events
- `soroban_pulse_eventbridge_put_events_failures_total` - Counter of failed submission attempts
- `soroban_pulse_eventbridge_rules_created_total` - Counter of created rules
- `soroban_pulse_eventbridge_rules_deleted_total` - Counter of deleted rules
- `soroban_pulse_eventbridge_active_rules` - Gauge of active rules

## Cross-Account Support

Enable sending events to EventBridge in another AWS account:

```rust
let config = EventBridgeConfig {
    cross_account_role_arn: Some(
        "arn:aws:iam::123456789012:role/CrossAccountEventBridgeRole".to_string()
    ),
    ..Default::default()
};
```

The integration will assume the specified role with appropriate trust relationships configured.

## Event Format

Events sent to EventBridge follow this structure:

```json
{
  "Source": "soroban-pulse",
  "DetailType": "SorobanEvent",
  "Detail": {
    "id": "event-123",
    "contract_id": "CABCDEF...",
    "event_type": "Transfer",
    "tx_hash": "abc123...",
    "ledger_close_time": 1234567890,
    "...": "... other event fields ..."
  },
  "EventBusName": "soroban-events"
}
```

## Routing to Targets

Once events are in EventBridge, use rules to route to targets:

### Lambda

Process events with custom logic:

```json
{
  "Name": "process-transfers",
  "EventPattern": {
    "source": ["soroban-pulse"],
    "detail-type": ["SorobanEvent"],
    "detail": {"event_type": ["Transfer"]}
  }
}
```

Target: Lambda function for custom processing

### SNS

Send notifications for critical events:

```json
{
  "Name": "critical-events-alert",
  "EventPattern": {
    "source": ["soroban-pulse"],
    "detail": {"severity": ["critical"]}
  }
}
```

Target: SNS topic for team notifications

### SQS

Queue events for batch processing:

```json
{
  "Name": "events-to-queue",
  "EventPattern": {
    "source": ["soroban-pulse"]
  }
}
```

Target: SQS queue for downstream processing

### Kinesis Data Streams

Stream events to real-time analytics:

```json
{
  "Name": "events-to-kinesis",
  "EventPattern": {
    "source": ["soroban-pulse"]
  }
}
```

Target: Kinesis stream for real-time analytics

## Performance Considerations

- **Batch submission**: Events are submitted in batches (default: 10) for efficiency
- **Event filtering**: Filter events before submission to reduce API calls
- **Timeout configuration**: Adjust timeout based on network latency to AWS
- **Async operation**: Event submission is non-blocking

## IAM Permissions

Required permissions for basic operation:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Action": [
        "events:PutEvents",
        "events:PutRule",
        "events:DeleteRule",
        "events:DescribeRule"
      ],
      "Resource": "arn:aws:events:*:*:rule/*"
    }
  ]
}
```

For cross-account access, add to the trusted account:

```json
{
  "Version": "2012-10-17",
  "Statement": [
    {
      "Effect": "Allow",
      "Principal": {
        "AWS": "arn:aws:iam::SOURCE_ACCOUNT:role/SorobanPulseRole"
      },
      "Action": "sts:AssumeRole"
    }
  ]
}
```

## Troubleshooting

### Connection Issues

If unable to reach EventBridge:
1. Verify AWS credentials are configured correctly
2. Check IAM permissions for `events:PutEvents`
3. Ensure the event bus exists and is accessible

### Failed Event Submissions

If events fail to submit:
1. Check the error in the response
2. Verify event format matches expectations
3. Review CloudTrail logs for API errors

### Rule Creation/Update Failures

If rules fail to create:
1. Validate event pattern JSON syntax
2. Check rule name uniqueness within event bus
3. Verify IAM permissions for `events:PutRule`

## Testing

The module includes comprehensive tests and mock implementations:

```bash
cargo test eventbridge
```

Use the mock publisher in tests:

```rust
use soroban_pulse::eventbridge::mock::MockEventBridgePublisher;

let publisher = MockEventBridgePublisher::new();
// Use in tests without AWS access
```

## Integration with Other AWS Services

### Step Functions

Orchestrate complex event processing workflows:

1. Create a state machine that processes Soroban events
2. Add EventBridge rule targeting Step Functions
3. Define state transitions based on event properties

### DynamoDB

Store event data for analysis:

1. Use Lambda as EventBridge target
2. Lambda writes events to DynamoDB
3. Query historical event data

### S3

Archive events to S3:

1. Create EventBridge rule targeting Kinesis
2. Kinesis Firehose delivers to S3
3. Use S3 Select for ad-hoc queries

### CloudWatch Logs

Monitor events in CloudWatch:

1. Use EventBridge rule with CloudWatch Logs target
2. Create metric filters for important events
3. Set up alarms on event patterns
