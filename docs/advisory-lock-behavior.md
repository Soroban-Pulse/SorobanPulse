# Advisory Lock Behavior & Leader Election Race Fix

`src/advisory_lock.rs` wraps Postgres advisory locks for single-leader
operations (e.g. running migrations, coordinating multi-replica indexers).

## The Race Condition

Two related races existed in the previous implementation:

1. **Broken acquisition query.** The old code called a non-existent
   function, `pg_advisory_lock_acquire($1, 0, $timeout)`, instead of the
   real `pg_try_advisory_lock($1)`. Every acquisition attempt would fail
   with a database error.
2. **Ambiguous state under network delay.** `acquire()` wrapped each
   attempt in `tokio::time::timeout`. If the client-side timeout fired
   while the query was still in flight on the server, the client treated
   the attempt as failed and retried — but the original query could still
   complete and acquire the lock server-side. Because Postgres
   session-level advisory locks are **reentrant**, the next retry on the
   same connection would also report success, so the instance believed it
   had acquired the lock twice. A single `release()` call only undoes one
   level of reentrancy, leaving the lock held indefinitely — exactly the
   kind of split-brain/stuck-leader scenario that shows up as duplicated
   work or a stuck failover during leader election under network jitter.

## The Fix

- **Correct SQL**: acquisition now uses the real, non-blocking
  `pg_try_advisory_lock($1)`, which never leaves an ambiguous "maybe
  acquired" state — a call either returns immediately with `true`/`false`
  or errors; there is no blocking window for a delayed response to land
  after a client-side timeout.
- **In-process acquisition guard** (`AdvisoryLock::acquiring: AtomicBool`):
  `acquire()` now atomically claims the right to attempt acquisition via
  `compare_exchange`. A second concurrent call on the same instance (e.g. a
  retry racing the original attempt) is rejected immediately with
  `AdvisoryLockError::ConcurrentAcquisitionInProgress` instead of being
  allowed to race.
- **Fencing tokens** (`AdvisoryLock::epoch: AtomicU64`): every successful
  acquisition returns a `LockGuard { lock_id, fencing_token }`. Callers
  should re-validate their token via `AdvisoryLock::is_current(token)`
  immediately before performing leader-only side effects, and `release()`
  requires the caller's `LockGuard` and refuses to release if the token no
  longer matches the current epoch (`AdvisoryLockError::StaleFencingToken`).
  This is the standard fencing-token pattern for lease/lock systems: a
  leader whose success response was delayed by the network — and who has
  since been superseded — can detect staleness instead of acting as if it
  were still authoritative.

## API Changes

- `AdvisoryLock::acquire(conn) -> Result<LockGuard, AdvisoryLockError>`
  (previously returned `Result<(), _>`).
- `AdvisoryLock::release(conn, guard: LockGuard) -> Result<(), AdvisoryLockError>`
  (previously took no guard argument).
- `AdvisoryLock::is_current(token: u64) -> bool` — new; check before
  committing leader-only side effects.

## Tests

- `advisory_lock.rs` unit tests: guard rejection, fencing-token
  increment/staleness detection, and a concurrency stress test spawning 50
  tasks racing for the acquisition guard.
- `tests/test_advisory_lock.rs`: existing DB-backed regression/concurrency
  tests covering multi-replica acquisition against a real Postgres
  instance.
