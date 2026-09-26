-- Down migration for 20260830000004_scheduled_event_replay.sql
DROP INDEX IF EXISTS idx_replay_schedules_subscription;
DROP INDEX IF EXISTS idx_replay_schedules_next_run;
DROP INDEX IF EXISTS idx_scheduled_replay_runs_schedule;
DROP TABLE IF EXISTS replay_schedules CASCADE;
DROP TABLE IF EXISTS scheduled_replay_runs CASCADE;
