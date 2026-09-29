/**
 * AUTO-GENERATED — do not edit by hand.
 * Regenerate with: npm run gen:api
 *
 * Source: openapi.json (Soroban Pulse API v1.0.0)
 */

export interface paths {
  "/health": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    /** @description Service is healthy */
    get: operations["health"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/status": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    /** @description Indexer operational status */
    get: operations["status"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/contracts": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_contracts"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/events": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_events"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/events/contract/{contract_id}": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_events_by_contract"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/events/contract/{contract_id}/stream": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["stream_events_by_contract"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/events/stream": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["stream_events"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/events/tx/{tx_hash}": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_events_by_tx"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/events/tx/{tx_hash}/related": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_related_events_by_tx"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/contracts/{contract_id}/stats/history": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_contract_stats_history"];
    put?: never;
    post?: never;
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
  "/v1/admin/contracts/{contract_id}/abi": {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    get: operations["get_contract_abi"];
    put?: never;
    post: operations["register_contract_abi"];
    delete?: never;
    options?: never;
    head?: never;
    patch?: never;
    trace?: never;
  };
}

export type webhooks = Record<string, never>;

export interface components {
  schemas: {
    /** @description A Soroban smart-contract event */
    Event: {
      /** @description Stellar contract ID */
      contract_id: string;
      /** Format: date-time */
      created_at: string;
      /** @description Arbitrary decoded event payload */
      event_data: unknown;
      event_type: components["schemas"]["EventType"];
      /**
       * Format: uuid
       * @description Unique event identifier
       */
      id: string;
      /** Format: int64 */
      ledger: number;
      /** Format: date-time */
      timestamp: string;
      /** @description Transaction hash */
      tx_hash: string;
    };
    /**
     * @description Soroban event category
     * @enum {string}
     */
    EventType: "contract" | "diagnostic" | "system";
    ContractSummary: {
      contract_id: string;
      /** Format: int64 */
      event_count: number;
      /** Format: int64 */
      latest_ledger: number;
    };
    PaginationParams: {
      cursor?: string | null;
      event_type?: components["schemas"]["EventType"] | null;
      exact_count?: boolean | null;
      fields?: string | null;
      /** Format: int64 */
      from_ledger?: number | null;
      /** Format: int64 */
      limit?: number | null;
      /** Format: int64 */
      page?: number | null;
      /** Format: int64 */
      to_ledger?: number | null;
      /** @enum {string} */
      sort?: "asc" | "desc" | null;
      /** @enum {string} */
      sort_by?: "ledger" | "timestamp" | "created_at" | null;
    };
    /** @description RFC 9457 Problem Detail */
    ProblemDetail: {
      /** @description URI identifying the problem type */
      type: string;
      /** @description Short, human-readable summary */
      title: string;
      /** @description HTTP status code */
      status: number;
      /** @description Human-readable explanation */
      detail?: string;
      /** @description URI of the specific occurrence */
      instance?: string;
    };
    /** @description Paginated wrapper returned by list endpoints */
    PaginatedResponse: {
      data: components["schemas"]["Event"][];
      /** @description Total matching records (approximate unless exact_count=true) */
      total: number;
      page: number;
      limit: number;
      /** @description Cursor for the next page (keyset pagination) */
      next_cursor?: string | null;
    };
    /** @description Response from /status */
    StatusResponse: {
      /** @description Whether the indexer is actively processing */
      indexing: boolean;
      /** @description Latest ledger sequence number ingested */
      latest_ledger: number;
      /** @description ISO-8601 timestamp of last successful ledger */
      last_ingested_at?: string | null;
    };
    ContractStatsEntry: {
      /** Format: date */
      date: string;
      /** Format: int64 */
      event_count: number;
      /** Format: int64 */
      unique_tx_count: number;
    };
  };
  responses: never;
  parameters: never;
  requestBodies: never;
  headers: never;
  pathItems: never;
}

export type $defs = Record<string, never>;

export interface operations {
  health: {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    requestBody?: never;
    responses: {
      /** @description Service is healthy */
      200: { headers: Record<string, unknown>; content?: never };
      /** @description Service is degraded */
      503: { headers: Record<string, unknown>; content?: never };
    };
  };
  status: {
    parameters: { query?: never; header?: never; path?: never; cookie?: never };
    requestBody?: never;
    responses: {
      /** @description Indexer operational status */
      200: {
        headers: Record<string, unknown>;
        content: { "application/json": components["schemas"]["StatusResponse"] };
      };
    };
  };
  get_contracts: {
    parameters: {
      query?: {
        /** @description Page number (default 1) */
        page?: number | null;
        /** @description Items per page (1-100, default 20) */
        limit?: number | null;
      };
      header?: never;
      path?: never;
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      /** @description Paginated list of indexed contract IDs */
      200: {
        headers: Record<string, unknown>;
        content: {
          "application/json": {
            data: string[];
            total: number;
            page: number;
            limit: number;
          };
        };
      };
    };
  };
  get_events: {
    parameters: {
      query?: {
        /** @description Page number (default: 1) */
        page?: number | null;
        /** @description Results per page, 1–100 (default: 20) */
        limit?: number | null;
        /** @description Opaque cursor for keyset pagination */
        cursor?: string | null;
        /** @description Use exact COUNT(*) instead of approximate */
        exact_count?: boolean | null;
        /** @description Filter by event type: contract, diagnostic, system */
        event_type?: components["schemas"]["EventType"] | null;
        /** @description Filter by contract ID */
        contract_id?: string | null;
        /** @description Return events at or after this ledger */
        from_ledger?: number | null;
        /** @description Return events at or before this ledger */
        to_ledger?: number | null;
        /** @description Filter by first topic symbol */
        topic_sym?: string | null;
        /** @description Full-text search query for event_data */
        search?: string | null;
        /** @description Filter events by ledger hash */
        ledger_hash?: string | null;
        /** @description Filter events by anonymized status (admin only) */
        anonymized?: boolean | null;
        /** @enum {string} */
        sort?: "asc" | "desc" | null;
        /** @enum {string} */
        sort_by?: "ledger" | "timestamp" | "created_at" | null;
      };
      header?: never;
      path?: never;
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      /** @description Paginated list of events */
      200: {
        headers: Record<string, unknown>;
        content: {
          "application/json": components["schemas"]["PaginatedResponse"];
        };
      };
      /** @description Invalid query parameters */
      400: {
        headers: Record<string, unknown>;
        content: {
          "application/problem+json": components["schemas"]["ProblemDetail"];
        };
      };
    };
  };
  get_events_by_contract: {
    parameters: {
      query?: {
        /** @description Page number (default: 1) */
        page?: number | null;
        /** @description Results per page, 1–100 (default: 20) */
        limit?: number | null;
        /** @description Opaque cursor for keyset pagination */
        cursor?: string | null;
        /** @enum {string} */
        sort?: "asc" | "desc" | null;
        /** @enum {string} */
        sort_by?: "ledger" | "timestamp" | "created_at" | null;
        /** @description Return events at or after this ledger */
        from_ledger?: number | null;
        /** @description Return events at or before this ledger */
        to_ledger?: number | null;
      };
      header?: never;
      path: {
        /** @description Stellar contract ID (56-char, starts with C) */
        contract_id: string;
      };
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      /** @description Events for the given contract */
      200: {
        headers: Record<string, unknown>;
        content: {
          "application/json": components["schemas"]["PaginatedResponse"];
        };
      };
      /** @description Invalid contract_id format or ledger range */
      400: {
        headers: Record<string, unknown>;
        content: {
          "application/problem+json": components["schemas"]["ProblemDetail"];
        };
      };
    };
  };
  stream_events_by_contract: {
    parameters: {
      query?: never;
      header?: never;
      path: {
        /** @description Stellar contract ID */
        contract_id: string;
      };
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      /** @description SSE stream of contract events (text/event-stream) */
      200: { headers: Record<string, unknown>; content?: never };
      /** @description Invalid contract_id format */
      400: { headers: Record<string, unknown>; content?: never };
    };
  };
  stream_events: {
    parameters: {
      query?: {
        /** @description Filter by contract ID (less preferred) */
        contract_id?: string | null;
      };
      header?: never;
      path?: never;
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      /** @description SSE stream of new events (text/event-stream) */
      200: { headers: Record<string, unknown>; content?: never };
      400: { headers: Record<string, unknown>; content?: never };
    };
  };
  get_events_by_tx: {
    parameters: {
      query?: never;
      header?: never;
      path: {
        /** @description Transaction hash (64 lowercase hex chars) */
        tx_hash: string;
      };
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      200: {
        headers: Record<string, unknown>;
        content: {
          "application/json": components["schemas"]["Event"][];
        };
      };
      400: {
        headers: Record<string, unknown>;
        content: {
          "application/problem+json": components["schemas"]["ProblemDetail"];
        };
      };
    };
  };
  get_related_events_by_tx: {
    parameters: {
      query?: {
        /** @description Reference traversal depth, default 1, max 3 */
        depth?: number | null;
      };
      header?: never;
      path: {
        /** @description Root transaction hash (64 hex chars) */
        tx_hash: string;
      };
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      200: {
        headers: Record<string, unknown>;
        content: {
          "application/json": components["schemas"]["Event"][];
        };
      };
      400: {
        headers: Record<string, unknown>;
        content: {
          "application/problem+json": components["schemas"]["ProblemDetail"];
        };
      };
    };
  };
  get_contract_stats_history: {
    parameters: {
      query?: {
        /** @description Aggregation bucket (only "1d" supported) */
        bucket?: string | null;
        /** @description Number of daily buckets to return */
        days?: number | null;
        /** @description Start date, YYYY-MM-DD */
        from?: string | null;
        /** @description End date, YYYY-MM-DD */
        to?: string | null;
      };
      header?: never;
      path: {
        /** @description Stellar contract ID */
        contract_id: string;
      };
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      200: {
        headers: Record<string, unknown>;
        content: {
          "application/json": components["schemas"]["ContractStatsEntry"][];
        };
      };
      400: {
        headers: Record<string, unknown>;
        content: {
          "application/problem+json": components["schemas"]["ProblemDetail"];
        };
      };
    };
  };
  get_contract_abi: {
    parameters: {
      query?: never;
      header?: never;
      path: {
        /** @description Stellar contract ID */
        contract_id: string;
      };
      cookie?: never;
    };
    requestBody?: never;
    responses: {
      200: {
        headers: Record<string, unknown>;
        content: { "application/json": unknown };
      };
      404: { headers: Record<string, unknown>; content?: never };
    };
  };
  register_contract_abi: {
    parameters: {
      query?: never;
      header?: never;
      path: {
        /** @description Stellar contract ID */
        contract_id: string;
      };
      cookie?: never;
    };
    requestBody: {
      content: { "application/json": unknown };
    };
    responses: {
      200: { headers: Record<string, unknown>; content?: never };
      400: {
        headers: Record<string, unknown>;
        content: {
          "application/problem+json": components["schemas"]["ProblemDetail"];
        };
      };
    };
  };
}
