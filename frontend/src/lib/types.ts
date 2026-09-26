// Response shapes of the Soroban Pulse API endpoints used by the UI.

export interface EventRecord {
  id: string;
  contract_id: string;
  event_type: string;
  tx_hash: string;
  ledger: number;
  timestamp: string;
  event_data: unknown;
  event_data_decoded?: unknown;
  in_successful_call?: boolean;
}

export interface EventPage {
  data: EventRecord[];
  next_cursor?: string | null;
  total?: number;
}

export interface ContractSearchResult {
  contract_id: string;
  event_count: number;
  last_event_at: string | null;
}

export interface IndexerStatus {
  version: string;
  uptime_secs: number;
  current_ledger: number;
  latest_ledger: number;
  lag_ledgers: number;
  total_events: number;
  events_by_type: Record<string, number>;
  indexer_status: "running" | "stalled";
  indexer_mode: "active" | "read_only";
  indexer_paused: boolean;
}

export interface PublicConfig {
  indexer_lag_warn_threshold: number;
  rate_limit_per_minute?: number;
}

export type HealthLevel = "ok" | "degraded" | "unhealthy";

export interface ComponentHealth {
  status: HealthLevel;
  message?: string | null;
  response_time_ms?: number;
  pool_size?: number | null;
  idle_connections?: number | null;
  active_endpoint?: string | null;
  service?: string;
}

export interface OverallHealth {
  status: "ok" | "degraded";
  db?: string;
  indexer?: string;
  last_poll_secs_ago?: number;
}

export interface SloReport {
  name: string;
  description: string;
  component: string;
  sli_type: string;
  target: number;
  window_secs: number;
  completion_ratio: number;
  error_budget_remaining: number;
  burn_rate: number;
  status: "met" | "at_risk" | "breached";
  sample_count: number;
}

export interface SloAggregateReport {
  generated_at: number;
  slos: SloReport[];
  counts: { met: number; at_risk: number; breached: number };
}

export interface AuditLogEntry {
  id: string;
  event_type: string | null;
  action: string | null;
  resource_type: string | null;
  resource_id: string | null;
  request_path: string | null;
  request_method: string | null;
  user_email: string | null;
  created_by: string | null;
  status_code: number | null;
  success: boolean | null;
  severity: string | null;
  created_at: string;
}

export interface LedgerGap {
  from_ledger: number;
  to_ledger: number;
  reason?: string;
}
