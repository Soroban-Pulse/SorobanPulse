-- Down migration for 20260527024126_add_composite_indexes.sql
DROP INDEX IF EXISTS idx_events_contract_type_ledger;
DROP INDEX IF EXISTS idx_events_type_ledger;
DROP INDEX IF EXISTS idx_events_contract_type_partial;
