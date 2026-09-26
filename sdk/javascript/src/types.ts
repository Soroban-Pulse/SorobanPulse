export interface SorobanPulseClientOptions {
  apiKey: string;
  baseUrl?: string;
  timeoutMs?: number;
  maxRetries?: number;
}

export interface SorobanEvent {
  id: string;
  contractId: string;
  eventType: string;
  ledger: number;
  txHash: string;
  data: Record<string, unknown>;
  createdAt: string;
}

export interface Page<T> {
  data: T[];
  nextCursor: string | null;
}

export interface ListEventsParams {
  contractId?: string;
  eventType?: string;
  limit?: number;
  cursor?: string;
}

export interface Subscription {
  id: string;
  contractId: string;
  webhookUrl: string;
  eventTypes: string[];
  createdAt: string;
}

export interface CreateSubscriptionParams {
  contractId: string;
  webhookUrl: string;
  eventTypes?: string[];
}

export type EventHandler = (event: SorobanEvent) => void;
