# Priority-Based Webhook Delivery

Soroban Pulse supports priority-based delivery for webhook events so that
critical events (security incidents, payment failures, outage alerts) are
delivered ahead of routine/low-priority notifications when the delivery
pipeline is under load.

Implementation: [`src/webhook_priority.rs`](../src/webhook_priority.rs).

## Priority Levels

| Priority | Max Wait SLA | Typical Use |
|----------|-------------|-------------|
| `critical` | 1s | Security breaches, payment failures |
| `high` | 5s | Elevated error rates, incident escalation |
| `normal` | 30s | Standard event notifications (default) |
| `low` | 120s | Digest/summary notifications |

## Priority Queue

`WebhookPriorityQueue` is a thread-safe max-heap keyed on
`(priority, insertion order)`. Within the same priority level, delivery is
FIFO. Pushing a task assigns a monotonic sequence number used purely as a
tiebreaker so ordering is deterministic.

```rust
use soroban_pulse::webhook_priority::{WebhookDeliveryTask, WebhookPriority, WebhookPriorityQueue};

let queue = WebhookPriorityQueue::new();
queue.push(WebhookDeliveryTask::new(url, payload, WebhookPriority::Critical));

while let Some(task) = queue.pop() {
    // deliver task.payload to task.webhook_url
}
```

## Configurable Priority Rules

`PriorityRuleSet` maps event type patterns (with trailing `*` wildcard
support) to a priority level. Rules are evaluated in order; the first match
wins, falling back to a configured default:

```rust
use soroban_pulse::webhook_priority::{PriorityRuleSet, WebhookPriority};

let mut rules = PriorityRuleSet::new(WebhookPriority::Normal);
rules.add_rule("security.*", WebhookPriority::Critical);
rules.add_rule("payment.failed", WebhookPriority::High);
```

## Metrics

Every dequeue records:

- `soroban_pulse_webhook_priority_dequeued_total{priority}` — delivery count per priority
- `soroban_pulse_webhook_priority_wait_ms{priority}` — histogram of queue wait time
- `soroban_pulse_webhook_priority_violations_total{priority}` — count of tasks that exceeded their priority's max-wait SLA

## Priority Violation Alerts

A task is considered a violation once `wait_ms() > priority.max_wait_ms()`.
`WebhookPriorityQueue::violating_tasks()` can be polled on an interval to
surface currently-violating task IDs for alerting (e.g. paging on-call when a
`critical` task has been queued for more than its 1s SLA).
