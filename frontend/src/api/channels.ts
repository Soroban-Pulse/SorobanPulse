import { request } from './client';
import type {
  ChannelDeliveryPoint,
  ChannelGroup,
  ChannelHealth,
  ChannelInput,
  ChannelTestResult,
  MaintenanceWindow,
  NotificationChannel,
  NotificationDashboard,
  Paged,
} from './types';

const BASE = '/v1/admin/notifications';

export interface ChannelQuery {
  q?: string;
  channel_type?: string;
  status?: string;
  tag?: string;
  page?: number;
  page_size?: number;
}

export const channelsApi = {
  list: (query: ChannelQuery = {}) =>
    request<Paged<NotificationChannel>>(`${BASE}/channels`, { admin: true, query: { ...query } }),

  create: (input: ChannelInput) =>
    request<NotificationChannel>(`${BASE}/channels`, { admin: true, method: 'POST', body: input }),

  update: (id: string, input: Partial<ChannelInput>) =>
    request<NotificationChannel>(`${BASE}/channels/${id}`, { admin: true, method: 'PATCH', body: input }),

  setEnabled: (id: string, enabled: boolean) =>
    request<NotificationChannel>(`${BASE}/channels/${id}`, {
      admin: true,
      method: 'PATCH',
      body: { status: enabled ? 'active' : 'disabled' },
    }),

  remove: (id: string) => request<void>(`${BASE}/channels/${id}`, { admin: true, method: 'DELETE' }),

  test: (id: string) =>
    request<ChannelTestResult>(`${BASE}/channels/${id}/test`, { admin: true, method: 'POST' }),

  health: () => request<Paged<ChannelHealth>>(`${BASE}/channels/health`, { admin: true }),

  deliveryStats: (id: string, period: '24h' | '7d' | '30d' = '7d') =>
    request<Paged<ChannelDeliveryPoint>>(`${BASE}/channels/${id}/stats`, { admin: true, query: { period } }),

  dashboard: () => request<NotificationDashboard>(`${BASE}/dashboard`, { admin: true }),

  groups: () => request<Paged<ChannelGroup>>(`${BASE}/channel-groups`, { admin: true }),

  maintenanceWindows: () => request<Paged<MaintenanceWindow>>(`${BASE}/maintenance-windows`, { admin: true }),

  createMaintenanceWindow: (input: {
    start_time: string;
    end_time: string;
    contract_ids: string[];
    description?: string;
  }) =>
    request<MaintenanceWindow>(`${BASE}/maintenance-windows`, { admin: true, method: 'POST', body: input }),

  deleteMaintenanceWindow: (id: string) =>
    request<void>(`${BASE}/maintenance-windows/${id}`, { admin: true, method: 'DELETE' }),
};
