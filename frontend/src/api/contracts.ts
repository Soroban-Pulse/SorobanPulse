import { request } from './client';
import type {
  ContractInfo,
  ContractSummary,
  EventsPage,
  Paged,
  StatsHistoryPoint,
  TimeseriesBucket,
  WasmVersion,
} from './types';

export const contractsApi = {
  summary: (id: string) => request<ContractSummary>(`/v1/contracts/${id}/summary`),

  // /v1/contracts/search returns label and metadata when registered.
  info: async (id: string): Promise<ContractInfo> => {
    const res = await request<Paged<ContractInfo>>('/v1/contracts/search', { query: { q: id, limit: 1 } });
    return res.data?.find((c) => c.contract_id === id) ?? { contract_id: id };
  },

  timeseries: (id: string, bucket: '1h' | '1d', fromLedger?: number) =>
    request<{ bucket: string; data: TimeseriesBucket[] }>('/v1/events/timeseries', {
      query: { contract_id: id, bucket, from_ledger: fromLedger },
    }),

  statsHistory: (id: string, days: number) =>
    request<{ contract_id: string; bucket: string; data: StatsHistoryPoint[] }>(
      `/v1/contracts/${id}/stats/history`,
      { query: { days, bucket: '1d' } },
    ),

  events: (
    id: string,
    query: { page?: number; limit?: number; event_type?: string; from_ledger?: number; to_ledger?: number },
  ) =>
    // exact_count=false keeps large contracts fast: the count comes from planner stats.
    request<EventsPage>(`/v1/events/contract/${id}`, { query: { ...query, exact_count: false } }),

  wasmVersions: (id: string) => request<Paged<WasmVersion>>(`/v1/contracts/${id}/wasm-versions`),
};
