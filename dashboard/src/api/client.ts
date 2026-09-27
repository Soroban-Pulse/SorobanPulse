export interface SystemStatus {
  status: "healthy" | "degraded" | "down";
  uptimeSeconds: number;
  version: string;
}

import type { Event, EventListResponse, EventFilterParams } from "./eventTypes";

export type { Event, EventListResponse, EventFilterParams };

export interface MetricPoint {
  timestamp: string;
  eventsIngested: number;
  latencyMsP99: number;
}

export interface SubscriptionSummary {
  id: string;
  contractId: string;
  webhookUrl: string;
  eventTypes: string[];
  status: "active" | "paused" | "failing";
}

export interface WebhookDelivery {
  id: string;
  subscriptionId: string;
  statusCode: number;
  attempt: number;
  deliveredAt: string;
}

export interface ApiErrorDetail {
  status: number;
  statusText: string;
  requestId: string | null;
  retryAfter: number | null;
  message: string;
}

export class ApiError extends Error {
  public readonly status: number;
  public readonly statusText: string;
  public readonly requestId: string | null;
  public readonly retryAfter: number | null;

  constructor(detail: ApiErrorDetail) {
    super(detail.message);
    this.name = "ApiError";
    this.status = detail.status;
    this.statusText = detail.statusText;
    this.requestId = detail.requestId;
    this.retryAfter = detail.retryAfter;
  }
}

function authHeaders(): Record<string, string> {
  const raw = localStorage.getItem("sorobanpulse.dashboard.auth");
  const token = raw ? JSON.parse(raw).token : null;
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function get<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, { headers: authHeaders() });
  if (!response.ok) {
    const requestId = response.headers.get("x-request-id");
    const retryAfter = response.headers.get("retry-after");
    const retryAfterSec = retryAfter ? Number(retryAfter) : null;
    const body = await response.text();
    throw new ApiError({
      status: response.status,
      statusText: response.statusText,
      requestId,
      retryAfter: retryAfterSec,
      message: body || `request to ${path} failed: ${response.status}`,
    });
  }
  return response.json();
}

export const dashboardApi = {
  getSystemStatus: () => get<SystemStatus>("/status"),
  getMetrics: (rangeMinutes = 60) => get<MetricPoint[]>(`/metrics?range=${rangeMinutes}`),
  listSubscriptions: () => get<SubscriptionSummary[]>("/subscriptions"),
  listWebhookDeliveries: (subscriptionId: string) =>
    get<WebhookDelivery[]>(`/subscriptions/${subscriptionId}/deliveries`),
  listEvents: (params: EventFilterParams = {}) => {
    const searchParams = new URLSearchParams();
    if (params.search) searchParams.set("search", params.search);
    if (params.eventType) searchParams.set("event_type", params.eventType);
    if (params.contractId) searchParams.set("contract_id", params.contractId);
    if (params.contractIdPrefix) searchParams.set("contract_id_prefix", params.contractIdPrefix);
    if (params.fromLedger != null) searchParams.set("from_ledger", String(params.fromLedger));
    if (params.toLedger != null) searchParams.set("to_ledger", String(params.toLedger));
    if (params.topic) searchParams.set("topic", params.topic);
    if (params.fromTimestamp) searchParams.set("from_timestamp", params.fromTimestamp);
    if (params.toTimestamp) searchParams.set("to_timestamp", params.toTimestamp);
    if (params.sortBy) searchParams.set("sort_by", params.sortBy);
    if (params.sortOrder) searchParams.set("sort_order", params.sortOrder);
    if (params.page != null) searchParams.set("page", String(params.page));
    if (params.limit != null) searchParams.set("limit", String(params.limit));
    const qs = searchParams.toString();
    return get<EventListResponse>(`/events${qs ? `?${qs}` : ""}`);
  },
  getEventById: (id: string) => get<Event>(`/events/${id}`),
};
