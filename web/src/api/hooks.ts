/**
 * TanStack Query hooks that wrap the typed openapi-fetch client.
 *
 * Each hook:
 *  - Has fully inferred request/response types derived from schema.d.ts
 *  - Propagates ApiError so consumers can check .status / .problem
 *  - Follows the stale-while-revalidate pattern via sensible staleTime defaults
 */

import {
  useQuery,
  useInfiniteQuery,
  type UseQueryOptions,
  type UseInfiniteQueryOptions,
  type InfiniteData,
} from "@tanstack/react-query";
import { apiClient, ApiError } from "./client.ts";
import type { components, operations } from "./schema.d.ts";

// ---------------------------------------------------------------------------
// Convenience aliases
// ---------------------------------------------------------------------------

export type Event = components["schemas"]["Event"];
export type EventType = components["schemas"]["EventType"];
export type PaginatedResponse = components["schemas"]["PaginatedResponse"];
export type StatusResponse = components["schemas"]["StatusResponse"];
export type ContractStatsEntry = components["schemas"]["ContractStatsEntry"];

// ---------------------------------------------------------------------------
// Query-key factory — keeps keys consistent and refactorable
// ---------------------------------------------------------------------------

export const queryKeys = {
  status: () => ["status"] as const,
  events: (params: GetEventsParams) => ["events", params] as const,
  eventsByContract: (contractId: string, params: GetEventsByContractParams) =>
    ["events", "contract", contractId, params] as const,
  eventsByTx: (txHash: string) => ["events", "tx", txHash] as const,
  contracts: (params: GetContractsParams) => ["contracts", params] as const,
  contract: (contractId: string) =>
    ["contracts", contractId] as const,
  contractStatsHistory: (
    contractId: string,
    params: GetContractStatsHistoryParams,
  ) => ["contracts", contractId, "stats", "history", params] as const,
};

// ---------------------------------------------------------------------------
// /status
// ---------------------------------------------------------------------------

/**
 * Polls the indexer status endpoint.
 *
 * @example
 * ```tsx
 * const { data, isLoading } = useStatus();
 * if (data?.indexing) console.log("latest ledger:", data.latest_ledger);
 * ```
 */
export function useStatus(
  options?: Partial<UseQueryOptions<StatusResponse, ApiError>>,
) {
  return useQuery<StatusResponse, ApiError>({
    queryKey: queryKeys.status(),
    queryFn: async () => {
      const { data } = await apiClient.GET("/status");
      // /status returns 200 with a JSON body; data is typed as StatusResponse.
      return data as StatusResponse;
    },
    staleTime: 10_000, // re-fetch every 10 s in the background
    refetchInterval: 15_000,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// /v1/events
// ---------------------------------------------------------------------------

type GetEventsQuery =
  operations["get_events"]["parameters"]["query"];

export type GetEventsParams = NonNullable<GetEventsQuery>;

/**
 * Paginated list of all events with optional filters.
 *
 * @example
 * ```tsx
 * const { data } = useEvents({ event_type: "contract", limit: 50 });
 * data?.data.forEach(e => console.log(e.id, e.contract_id));
 * ```
 */
export function useEvents(
  params: GetEventsParams = {},
  options?: Partial<UseQueryOptions<PaginatedResponse, ApiError>>,
) {
  return useQuery<PaginatedResponse, ApiError>({
    queryKey: queryKeys.events(params),
    queryFn: async () => {
      const { data } = await apiClient.GET("/v1/events", {
        params: { query: params },
      });
      return data as PaginatedResponse;
    },
    staleTime: 5_000,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// /v1/events/contract/{contract_id}  — cursor-based infinite scroll
// ---------------------------------------------------------------------------

type GetEventsByContractQuery =
  operations["get_events_by_contract"]["parameters"]["query"];

export type GetEventsByContractParams = Omit<
  NonNullable<GetEventsByContractQuery>,
  "cursor"
>;

/**
 * Cursor-paginated events for a single contract.
 * Integrates with TanStack Query's `useInfiniteQuery` for infinite scroll.
 *
 * @example
 * ```tsx
 * const { data, fetchNextPage, hasNextPage } = useContractEvents(contractId);
 * ```
 */
export function useContractEvents(
  contractId: string,
  params: GetEventsByContractParams = {},
  options?: Partial<
    UseInfiniteQueryOptions<
      PaginatedResponse,
      ApiError,
      InfiniteData<PaginatedResponse>,
      PaginatedResponse,
      ReturnType<typeof queryKeys.eventsByContract>,
      string | null
    >
  >,
) {
  return useInfiniteQuery<
    PaginatedResponse,
    ApiError,
    InfiniteData<PaginatedResponse>,
    ReturnType<typeof queryKeys.eventsByContract>,
    string | null
  >({
    queryKey: queryKeys.eventsByContract(contractId, params),
    initialPageParam: null,
    getNextPageParam: (lastPage) => lastPage.next_cursor ?? null,
    queryFn: async ({ pageParam }) => {
      const { data } = await apiClient.GET(
        "/v1/events/contract/{contract_id}",
        {
          params: {
            path: { contract_id: contractId },
            query: { ...params, cursor: pageParam },
          },
        },
      );
      return data as PaginatedResponse;
    },
    staleTime: 5_000,
    ...options,
  });
}

/**
 * Alias kept for issue requirements: `useContract(contractId)` returns the
 * first page of events for that contract.
 *
 * For infinite scroll use `useContractEvents` instead.
 */
export function useContract(
  contractId: string,
  params: GetEventsByContractParams = {},
  options?: Partial<UseQueryOptions<PaginatedResponse, ApiError>>,
) {
  return useQuery<PaginatedResponse, ApiError>({
    queryKey: queryKeys.contract(contractId),
    queryFn: async () => {
      const { data } = await apiClient.GET(
        "/v1/events/contract/{contract_id}",
        {
          params: {
            path: { contract_id: contractId },
            query: params,
          },
        },
      );
      return data as PaginatedResponse;
    },
    staleTime: 5_000,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// /v1/events/tx/{tx_hash}
// ---------------------------------------------------------------------------

/**
 * Events for a specific transaction hash.
 */
export function useEventsByTx(
  txHash: string,
  options?: Partial<UseQueryOptions<Event[], ApiError>>,
) {
  return useQuery<Event[], ApiError>({
    queryKey: queryKeys.eventsByTx(txHash),
    queryFn: async () => {
      const { data } = await apiClient.GET("/v1/events/tx/{tx_hash}", {
        params: { path: { tx_hash: txHash } },
      });
      return data as Event[];
    },
    staleTime: 60_000, // tx data is immutable once confirmed
    ...options,
  });
}

// ---------------------------------------------------------------------------
// /v1/contracts
// ---------------------------------------------------------------------------

type GetContractsQuery = operations["get_contracts"]["parameters"]["query"];
export type GetContractsParams = NonNullable<GetContractsQuery>;

/**
 * Paginated list of indexed contract IDs.
 */
export function useContracts(
  params: GetContractsParams = {},
  options?: Partial<
    UseQueryOptions<
      { data: string[]; total: number; page: number; limit: number },
      ApiError
    >
  >,
) {
  return useQuery<
    { data: string[]; total: number; page: number; limit: number },
    ApiError
  >({
    queryKey: queryKeys.contracts(params),
    queryFn: async () => {
      const { data } = await apiClient.GET("/v1/contracts", {
        params: { query: params },
      });
      return data as { data: string[]; total: number; page: number; limit: number };
    },
    staleTime: 30_000,
    ...options,
  });
}

// ---------------------------------------------------------------------------
// /v1/contracts/{contract_id}/stats/history
// ---------------------------------------------------------------------------

type GetContractStatsHistoryQuery =
  operations["get_contract_stats_history"]["parameters"]["query"];

export type GetContractStatsHistoryParams =
  NonNullable<GetContractStatsHistoryQuery>;

/**
 * Daily stats history for a contract (event count + unique tx count per day).
 */
export function useContractStatsHistory(
  contractId: string,
  params: GetContractStatsHistoryParams = {},
  options?: Partial<UseQueryOptions<ContractStatsEntry[], ApiError>>,
) {
  return useQuery<ContractStatsEntry[], ApiError>({
    queryKey: queryKeys.contractStatsHistory(contractId, params),
    queryFn: async () => {
      const { data } = await apiClient.GET(
        "/v1/contracts/{contract_id}/stats/history",
        {
          params: {
            path: { contract_id: contractId },
            query: params,
          },
        },
      );
      return data as ContractStatsEntry[];
    },
    staleTime: 60_000,
    ...options,
  });
}
