import { request } from './client';
import type {
  Delivery,
  DeliveryDetail,
  DlqEntry,
  DlqFilter,
  DlqReplayJob,
  DlqReplayResult,
  Paged,
  Subscription,
  SubscriptionInput,
} from './types';

export interface SubscriptionQuery {
  status?: string;
  contract_id?: string;
  q?: string;
  page?: number;
  page_size?: number;
}

export const subscriptionsApi = {
  list: (query: SubscriptionQuery = {}) =>
    request<Paged<Subscription>>('/v1/subscriptions', { query: { ...query } }),

  get: (id: string) => request<Subscription>(`/v1/subscriptions/${id}`),

  create: (input: SubscriptionInput) =>
    request<Subscription>('/v1/subscriptions', { method: 'POST', body: input }),

  update: (id: string, input: Partial<SubscriptionInput>) =>
    request<Subscription>(`/v1/subscriptions/${id}`, { method: 'PATCH', body: input }),

  updateBatch: (id: string, input: Pick<SubscriptionInput, 'subscription_type' | 'batch_size' | 'batch_timeout_ms'>) =>
    request<unknown>(`/v1/subscriptions/${id}/batch`, { method: 'PUT', body: input }),

  pause: (id: string) => request<unknown>(`/v1/subscriptions/${id}/pause`, { method: 'POST', body: {} }),

  resume: (id: string) => request<unknown>(`/v1/subscriptions/${id}/resume`, { method: 'POST' }),

  remove: (id: string) => request<void>(`/v1/subscriptions/${id}`, { method: 'DELETE' }),

  sendTest: (id: string) =>
    request<{ delivery_id?: string; status?: string }>(`/v1/subscriptions/${id}/test`, { method: 'POST' }),

  deliveries: (id: string, query: { status?: string; page?: number; page_size?: number } = {}) =>
    request<Paged<Delivery>>(`/v1/subscriptions/${id}/deliveries`, { query: { ...query } }),

  delivery: (id: string, deliveryId: string) =>
    request<DeliveryDetail>(`/v1/subscriptions/${id}/deliveries/${deliveryId}`),

  redeliver: (id: string, deliveryId: string) =>
    request<Delivery>(`/v1/subscriptions/${id}/deliveries/${deliveryId}/redeliver`, { method: 'POST' }),
};

export const dlqApi = {
  list: (filter: DlqFilter & { page?: number; page_size?: number } = {}) =>
    request<Paged<DlqEntry>>('/v1/admin/webhooks/dlq', { admin: true, query: { ...filter } }),

  replay: (filter: DlqFilter, dryRun: boolean) =>
    request<DlqReplayResult>('/v1/admin/webhooks/dlq/replay', {
      admin: true,
      method: 'POST',
      body: { ...filter, dry_run: dryRun },
    }),

  replayIds: (ids: string[]) =>
    request<DlqReplayResult>('/v1/admin/webhooks/dlq/replay', { admin: true, method: 'POST', body: { ids } }),

  job: (jobId: string) => request<DlqReplayJob>(`/v1/admin/webhooks/dlq/replay/${jobId}`, { admin: true }),
};
