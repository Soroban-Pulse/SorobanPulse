// Shapes returned by the Soroban Pulse API. Field names mirror the Rust
// structs in src/models.rs, src/subscriptions.rs and src/notification_admin.rs.

export interface Paged<T> {
  data: T[];
  total?: number;
  page?: number;
  page_size?: number;
  next_cursor?: string | null;
}

// ── Notification channels ────────────────────────────────────────────────────

export type ChannelType =
  | 'slack'
  | 'discord'
  | 'telegram'
  | 'email'
  | 'sms'
  | 'pagerduty'
  | 'github'
  | 'webhook';

export type ChannelStatus = 'active' | 'disabled' | string;

export interface NotificationChannel {
  id: string;
  name: string;
  channel_type: ChannelType;
  config: Record<string, unknown>;
  retry_policy?: Record<string, unknown> | null;
  description?: string | null;
  tags: string[];
  status: ChannelStatus;
  contract_filter: string[];
  failover_channel_id?: string | null;
  created_at: string;
  updated_at: string;
}

export interface ChannelInput {
  name: string;
  channel_type: ChannelType;
  config: Record<string, unknown>;
  description?: string;
  tags: string[];
  contract_filter: string[];
  status?: ChannelStatus;
}

export interface ChannelTestResult {
  channel_id: string;
  channel_name: string;
  channel_type: string;
  success: boolean;
  subject: string;
}

export type HealthState = 'healthy' | 'unhealthy' | 'unknown';

export interface ChannelHealth {
  channel_id: string;
  healthy: boolean | null;
  checked_at?: string | null;
  error?: string | null;
}

export interface ChannelDeliveryPoint {
  bucket_start: string;
  sent: number;
  failed: number;
}

export interface ChannelGroup {
  id: string;
  name: string;
  description?: string | null;
  channel_ids: string[];
}

export interface MaintenanceWindow {
  id: string;
  start_time: string;
  end_time: string;
  contract_ids: string[];
  description?: string | null;
  created_at: string;
}

export interface NotificationDashboard {
  totals: { last_24h: number; last_7d: number; last_30d: number };
  channels: {
    channel_id: string;
    channel_name: string;
    channel_type: string;
    sent_30d: number;
    failed_30d: number;
    success_rate: number;
  }[];
}

// ── Subscriptions ────────────────────────────────────────────────────────────

export type SubscriptionStatus = 'active' | 'paused' | 'cancelled' | string;

export interface Subscription {
  id: string;
  callback_url: string;
  from_ledger: number;
  acked_ledger: number;
  status: SubscriptionStatus;
  created_at: string;
  subscription_type: 'single' | 'batch';
  batch_size: number;
  batch_timeout_ms: number;
  contract_ids?: string[];
  event_types?: string[];
  /** Only present in the create response; never returned again. */
  secret?: string;
  last_delivery?: {
    status: DeliveryStatus;
    status_code?: number | null;
    attempted_at: string;
  } | null;
}

export interface SubscriptionInput {
  callback_url: string;
  from_ledger: number;
  subscription_type: 'single' | 'batch';
  batch_size?: number;
  batch_timeout_ms?: number;
  contract_ids: string[];
  event_types: string[];
  secret?: string;
}

// ── Deliveries & DLQ ─────────────────────────────────────────────────────────

export type DeliveryStatus = 'success' | 'failed' | 'pending' | 'retrying';

export interface Delivery {
  id: string;
  subscription_id: string;
  event_id?: string | null;
  status: DeliveryStatus;
  status_code?: number | null;
  latency_ms?: number | null;
  attempt: number;
  attempted_at: string;
  error?: string | null;
}

export interface DeliveryDetail extends Delivery {
  request: { url: string; headers: Record<string, string>; body: string; truncated?: boolean };
  response?: { headers: Record<string, string>; body: string; truncated?: boolean } | null;
}

export interface DlqEntry {
  id: string;
  subscription_id?: string | null;
  url: string;
  failure_reason?: string | null;
  status_code?: number | null;
  attempts: number;
  created_at: string;
  last_attempt_at?: string | null;
}

export interface DlqFilter {
  endpoint_url?: string;
  failure_reason?: string;
  created_after?: string;
  created_before?: string;
  max_attempts?: number;
}

export interface DlqReplayResult {
  dry_run: boolean;
  matched: number;
  replayed: number;
  job_id?: string | null;
}

export interface DlqReplayJob {
  job_id: string;
  total: number;
  processed: number;
  succeeded: number;
  failed: number;
  done: boolean;
}

// ── Contracts & events ───────────────────────────────────────────────────────

export interface ContractSummary {
  contract_id: string;
  total_events: number;
  first_event_at: string | null;
  last_event_at: string | null;
  unique_tx_count: number;
  ledger_range: { min: number | null; max: number | null };
  event_type_breakdown: { contract: number; diagnostic: number; system: number };
  from_cache: boolean;
}

export interface ContractInfo {
  contract_id: string;
  label?: string | null;
  metadata?: Record<string, unknown> | null;
  /** Set when the contract is a Stellar Asset Contract. */
  sac_asset?: { code: string; issuer?: string | null } | null;
}

export interface TimeseriesBucket {
  bucket_start: string;
  event_count: number;
  contract_count: number;
  event_types: Record<string, number>;
}

export interface StatsHistoryPoint {
  date: string;
  event_count: number;
  unique_tx_count: number;
}

export interface EventRow {
  id: string;
  contract_id: string;
  event_type: string;
  tx_hash: string;
  ledger: number;
  timestamp: string;
  event_data: unknown;
}

export interface EventsPage {
  data: EventRow[];
  total?: number;
  page?: number;
  limit?: number;
  approximate?: boolean;
}

export interface WasmVersion {
  wasm_hash: string;
  ledger: number;
  upgraded_at: string;
  tx_hash?: string | null;
}
