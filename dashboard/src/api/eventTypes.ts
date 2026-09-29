export type EventType = "contract" | "diagnostic" | "system";

export interface Event {
  id: string;
  contractId: string;
  eventType: EventType;
  txHash: string;
  ledger: number;
  timestamp: string;
  eventData: Record<string, unknown>;
  eventDataNormalized?: Record<string, unknown>;
  eventDataDecoded?: Record<string, unknown>;
  ledgerHash?: string;
  inSuccessfulCall: boolean;
  createdAt: string;
  schemaVersion: number;
  anonymized: boolean;
  fingerprint?: string;
  tenantId: string;
  network: string;
}

export interface EventListResponse {
  events: Event[];
  page: number;
  limit: number;
  total: number;
  hasMore: boolean;
}

export interface EventFilterParams {
  search?: string;
  eventType?: EventType;
  contractId?: string;
  contractIdPrefix?: string;
  fromLedger?: number;
  toLedger?: number;
  topic?: string;
  topic0?: string;
  topic1?: string;
  topic2?: string;
  topic3?: string;
  fromTimestamp?: string;
  toTimestamp?: string;
  inSuccessfulCall?: boolean;
  sortBy?: "ledger" | "timestamp" | "created_at";
  sortOrder?: "asc" | "desc";
  page?: number;
  limit?: number;
}

export interface ScValNode {
  key: string;
  value: string;
  type: "string" | "number" | "boolean" | "object" | "array" | "bytes";
  children?: ScValNode[];
}
