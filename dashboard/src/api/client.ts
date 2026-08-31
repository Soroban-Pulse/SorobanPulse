export interface SystemStatus {
  status: "healthy" | "degraded" | "down";
  uptimeSeconds: number;
  version: string;
}

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

function authHeaders(): Record<string, string> {
  const raw = localStorage.getItem("sorobanpulse.dashboard.auth");
  const token = raw ? JSON.parse(raw).token : null;
  return token ? { Authorization: `Bearer ${token}` } : {};
}

async function get<T>(path: string): Promise<T> {
  const response = await fetch(`/api${path}`, { headers: authHeaders() });
  if (!response.ok) throw new Error(`request to ${path} failed: ${response.status}`);
  return response.json();
}

export const dashboardApi = {
  getSystemStatus: () => get<SystemStatus>("/status"),
  getMetrics: (rangeMinutes = 60) => get<MetricPoint[]>(`/metrics?range=${rangeMinutes}`),
  listSubscriptions: () => get<SubscriptionSummary[]>("/subscriptions"),
  listWebhookDeliveries: (subscriptionId: string) =>
    get<WebhookDelivery[]>(`/subscriptions/${subscriptionId}/deliveries`),
};
