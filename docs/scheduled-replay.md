# Scheduled Event Replay

_Issue #930_ — Recurring schedules that automatically trigger event replay
operations, so operators can configure continuous or periodic re-delivery of
historical events without manual intervention.

## Overview

The on-demand replay system (Issue #679, `src/event_replay.rs`) lets operators
replay events for a specific subscription starting from a ledger number or
timestamp. Scheduled replay extends this with **persistent schedules** that
fire automatically according to a configurable cadence.

Three schedule types are supported:

| Type | Behaviour |
|---|---|
| `interval` | Replay every N seconds after the previous run completes. |
| `cron` | Replay according to a cron expression. `next_run_at` is written by an external cron parser. |
| `one_shot` | Replay once at `next_run_at`, then deactivate. |

## Architecture

```
Scheduler poll loop
  │
  ├── get_due_schedules()  ← finds schedules with next_run_at ≤ NOW()
  │
  ├── For each due schedule:
  │     ├── event_replay::replay_from_timestamp(offset = replay_from_offset_secs)
  │     ├── record_schedule_run(status="completed", events_replayed=N)
  │     ├── calculate_next_run(schedule)  ← None for cron/one_shot
  │     └── update_next_run(pool, schedule_id, next)
  │           └── if one_shot: update_schedule_status(pool, id, is_active=false)
  │
  └── sleep(poll_interval)
```

## Data Model

```
replay_schedules
├── id                     UUID  PK
├── name                   TEXT
├── subscription_id        UUID  FK → subscriptions(id)
├── schedule_type          TEXT   -- "interval" | "cron" | "one_shot"
├── cron_expression        TEXT?  -- required for cron schedules
├── interval_secs          BIGINT? -- required for interval schedules
├── filter_contract_id     TEXT?  -- optional event filter
├── filter_event_type      TEXT?  -- optional event type filter
├── max_events             BIGINT? -- cap per run (NULL = no limit)
├── replay_from_offset_secs BIGINT -- seconds before run time to replay from
├── is_active              BOOLEAN
├── last_run_at            TIMESTAMPTZ?
├── next_run_at            TIMESTAMPTZ?
├── created_at             TIMESTAMPTZ
└── updated_at             TIMESTAMPTZ

scheduled_replay_runs
├── id              UUID  PK
├── schedule_id     UUID  FK → replay_schedules(id)
├── replay_id       UUID? FK → replay_status(id)  -- the underlying on-demand replay
├── status          TEXT  -- "running" | "completed" | "failed"
├── events_replayed BIGINT
├── error_message   TEXT?
├── started_at      TIMESTAMPTZ
└── completed_at    TIMESTAMPTZ?
```

## API (Rust)

All functions live in `src/scheduled_replay.rs`.

### Create a schedule

```rust
use soroban_pulse::scheduled_replay::{create_replay_schedule, CreateScheduleRequest};
use uuid::Uuid;

// Replay the last hour of events for a subscription every 30 minutes.
let req = CreateScheduleRequest {
    name: "hourly-contract-replay".to_string(),
    subscription_id: my_subscription_id,
    schedule_type: "interval".to_string(),
    cron_expression: None,
    interval_secs: Some(1800),          // 30 minutes
    filter_contract_id: Some("CABC...".to_string()),
    filter_event_type: None,
    max_events: Some(1000),
    replay_from_offset_secs: 3600,      // replay the last 1 hour
};

let schedule = create_replay_schedule(&pool, req).await?;
println!("Created schedule {}", schedule.id);
```

### List schedules for a subscription

```rust
use soroban_pulse::scheduled_replay::list_replay_schedules;

let schedules = list_replay_schedules(&pool, subscription_id).await?;
for s in &schedules {
    println!(
        "{} — {} ({}) active={} next_run={:?}",
        s.id, s.name, s.schedule_type, s.is_active, s.next_run_at
    );
}
```

### Poll for due schedules (scheduler loop)

```rust
use soroban_pulse::scheduled_replay::{
    get_due_schedules, record_schedule_run, calculate_next_run, update_next_run,
    update_schedule_status,
};

loop {
    let due = get_due_schedules(&pool).await?;

    for schedule in due {
        // Determine the replay window.
        let from_ts = Utc::now()
            - chrono::Duration::seconds(schedule.replay_from_offset_secs);

        // Run the underlying on-demand replay.
        let result = event_replay::replay_from_timestamp(
            &pool,
            schedule.subscription_id,
            from_ts,
            schedule.max_events.map(|n| n as i32),
            &ReplayConfig::default(),
            false,
        )
        .await;

        let (status, events, error, replay_id) = match result {
            Ok(resp) => ("completed", resp.total_events, None, Some(resp.replay_id)),
            Err(e)   => ("failed", 0, Some(e.as_str()), None),
        };

        record_schedule_run(
            &pool,
            schedule.id,
            replay_id,
            status,
            events,
            error,
        )
        .await?;

        // Advance next_run_at (None for cron/one_shot).
        let next = calculate_next_run(&schedule);
        update_next_run(&pool, schedule.id, next).await?;

        // Deactivate one-shot schedules after their single run.
        if schedule.schedule_type == "one_shot" {
            update_schedule_status(&pool, schedule.id, false).await?;
        }
    }

    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
}
```

### Deactivate / reactivate a schedule

```rust
use soroban_pulse::scheduled_replay::update_schedule_status;

// Pause
update_schedule_status(&pool, schedule_id, false).await?;

// Resume
update_schedule_status(&pool, schedule_id, true).await?;
```

## Struct Reference

### `ScheduleType`

```rust
pub enum ScheduleType {
    Interval,
    Cron,
    OneShot,
}
```

Use `ScheduleType::as_str()` to get the DB string representation, and
`ScheduleType::from_str(s)` to parse it back.

### `ReplaySchedule`

Mirrors the `replay_schedules` table row.

```rust
pub struct ReplaySchedule {
    pub id:                     Uuid,
    pub name:                   String,
    pub subscription_id:        Uuid,
    pub schedule_type:          String,
    pub cron_expression:        Option<String>,
    pub interval_secs:          Option<i64>,
    pub filter_contract_id:     Option<String>,
    pub filter_event_type:      Option<String>,
    pub max_events:             Option<i64>,
    pub replay_from_offset_secs: i64,
    pub is_active:              bool,
    pub last_run_at:            Option<DateTime<Utc>>,
    pub next_run_at:            Option<DateTime<Utc>>,
    pub created_at:             DateTime<Utc>,
}
```

### `CreateScheduleRequest`

Input to `create_replay_schedule`.

### `ScheduledReplayRun`

Mirrors the `scheduled_replay_runs` table row.

```rust
pub struct ScheduledReplayRun {
    pub id:              Uuid,
    pub schedule_id:     Uuid,
    pub replay_id:       Option<Uuid>,
    pub status:          String,
    pub events_replayed: i64,
    pub error_message:   Option<String>,
    pub started_at:      DateTime<Utc>,
    pub completed_at:    Option<DateTime<Utc>>,
}
```

## Indexes

| Index | Table | Columns | Purpose |
|---|---|---|---|
| `idx_replay_schedules_subscription` | `replay_schedules` | `(subscription_id, is_active)` | List schedules per subscription |
| `idx_replay_schedules_next_run` | `replay_schedules` | `(next_run_at, is_active) WHERE is_active` | Scheduler due-check poll |
| `idx_scheduled_replay_runs_schedule` | `scheduled_replay_runs` | `(schedule_id, started_at DESC)` | Run history per schedule |

## Schedule Type Reference

### `interval`

- `interval_secs` is required and must be > 0.
- After each run, `calculate_next_run` returns `NOW() + interval_secs`.
- `update_next_run` stores this value so the scheduler picks it up on the
  next poll.

### `cron`

- `cron_expression` is required (e.g. `"0 */6 * * *"` for every 6 hours).
- `calculate_next_run` returns `None` — the cron expression is evaluated by
  an external library that writes `next_run_at` directly before the first run.
- This project does not bundle a cron parser; integrate `cron` or
  `cron_clock` from crates.io if you need in-process cron evaluation.

### `one_shot`

- Runs exactly once when `next_run_at` arrives.
- `calculate_next_run` returns `None`.
- After recording the run, call
  `update_schedule_status(pool, id, false)` to deactivate the schedule.

## Caveats and Limitations

- **No distributed lock**: if multiple service replicas poll
  `get_due_schedules` simultaneously, the same schedule can trigger more than
  once. Use the existing advisory lock mechanism (see `src/advisory_lock.rs`)
  or add `SELECT … FOR UPDATE SKIP LOCKED` to `get_due_schedules` to prevent
  duplicate runs.
- **No cron parser included**: cron schedules require an external component to
  compute `next_run_at`. This is by design to avoid adding a new dependency.
- **Large replays**: if `max_events` is not set and the replay window is large,
  the run may exceed the `LARGE_REPLAY_THRESHOLD` (100 000 events) and require
  `approved=true`. Set `max_events` to cap per-run delivery volume.
