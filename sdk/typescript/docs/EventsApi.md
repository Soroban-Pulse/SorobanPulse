# EventsApi

All URIs are relative to *http://localhost*

| Method | HTTP request | Description |
|------------- | ------------- | -------------|
| [**getContracts**](EventsApi.md#getcontracts) | **GET** /v1/contracts |  |
| [**getEvents**](EventsApi.md#getevents) | **GET** /v1/events |  |
| [**getEventsByContract**](EventsApi.md#geteventsbycontract) | **GET** /v1/events/contract/{contract_id} |  |
| [**getEventsByTx**](EventsApi.md#geteventsbytx) | **GET** /v1/events/tx/{tx_hash} |  |
| [**streamEvents**](EventsApi.md#streamevents) | **GET** /v1/events/stream | Stream new events in real time via Server-Sent Events. |
| [**streamEventsByContract**](EventsApi.md#streameventsbycontract) | **GET** /v1/events/contract/{contract_id}/stream | Stream new events for a specific contract in real time via Server-Sent Events. |



## getContracts

> getContracts(page, limit)



### Example

```ts
import {
  Configuration,
  EventsApi,
} from '';
import type { GetContractsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new EventsApi();

  const body = {
    // number | Page number (default 1) (optional)
    page: 789,
    // number | Items per page (1-100, default 20) (optional)
    limit: 789,
  } satisfies GetContractsRequest;

  try {
    const data = await api.getContracts(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **page** | `number` | Page number (default 1) | [Optional] [Defaults to `undefined`] |
| **limit** | `number` | Items per page (1-100, default 20) | [Optional] [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Paginated list of indexed contract IDs |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getEvents

> getEvents(page, limit, exactCount, eventType, fromLedger, toLedger)



### Example

```ts
import {
  Configuration,
  EventsApi,
} from '';
import type { GetEventsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new EventsApi();

  const body = {
    // number | Page number (default: 1) (optional)
    page: 789,
    // number | Results per page, 1–100 (default: 20) (optional)
    limit: 789,
    // boolean | Use exact COUNT(*) instead of approximate (optional)
    exactCount: true,
    // EventType | Filter by event type: contract, diagnostic, system (optional)
    eventType: ...,
    // number | Return events at or after this ledger (optional)
    fromLedger: 789,
    // number | Return events at or before this ledger (optional)
    toLedger: 789,
  } satisfies GetEventsRequest;

  try {
    const data = await api.getEvents(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **page** | `number` | Page number (default: 1) | [Optional] [Defaults to `undefined`] |
| **limit** | `number` | Results per page, 1–100 (default: 20) | [Optional] [Defaults to `undefined`] |
| **exactCount** | `boolean` | Use exact COUNT(*) instead of approximate | [Optional] [Defaults to `undefined`] |
| **eventType** | `EventType` | Filter by event type: contract, diagnostic, system | [Optional] [Defaults to `undefined`] [Enum: contract, diagnostic, system] |
| **fromLedger** | `number` | Return events at or after this ledger | [Optional] [Defaults to `undefined`] |
| **toLedger** | `number` | Return events at or before this ledger | [Optional] [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Paginated list of events |  -  |
| **400** | Invalid query parameters |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getEventsByContract

> getEventsByContract(contractId, page, limit, fromLedger, toLedger)



### Example

```ts
import {
  Configuration,
  EventsApi,
} from '';
import type { GetEventsByContractRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new EventsApi();

  const body = {
    // string | Stellar contract ID (56-char, starts with C)
    contractId: contractId_example,
    // number | Page number (default: 1) (optional)
    page: 789,
    // number | Results per page, 1–100 (default: 20) (optional)
    limit: 789,
    // number | Return events at or after this ledger (optional)
    fromLedger: 789,
    // number | Return events at or before this ledger (optional)
    toLedger: 789,
  } satisfies GetEventsByContractRequest;

  try {
    const data = await api.getEventsByContract(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **contractId** | `string` | Stellar contract ID (56-char, starts with C) | [Defaults to `undefined`] |
| **page** | `number` | Page number (default: 1) | [Optional] [Defaults to `undefined`] |
| **limit** | `number` | Results per page, 1–100 (default: 20) | [Optional] [Defaults to `undefined`] |
| **fromLedger** | `number` | Return events at or after this ledger | [Optional] [Defaults to `undefined`] |
| **toLedger** | `number` | Return events at or before this ledger | [Optional] [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Events for the given contract |  -  |
| **400** | Invalid contract_id format or ledger range |  -  |
| **404** | No events found for contract |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## getEventsByTx

> getEventsByTx(txHash)



### Example

```ts
import {
  Configuration,
  EventsApi,
} from '';
import type { GetEventsByTxRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new EventsApi();

  const body = {
    // string | Transaction hash (64 lowercase hex chars)
    txHash: txHash_example,
  } satisfies GetEventsByTxRequest;

  try {
    const data = await api.getEventsByTx(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **txHash** | `string` | Transaction hash (64 lowercase hex chars) | [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | Events for the given transaction (empty array if none) |  -  |
| **400** | Invalid tx_hash format |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## streamEvents

> streamEvents(contractId)

Stream new events in real time via Server-Sent Events.

This endpoint is less preferred for contract-specific streaming; use &#x60;/v1/events/contract/{contract_id}/stream&#x60; instead.

### Example

```ts
import {
  Configuration,
  EventsApi,
} from '';
import type { StreamEventsRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new EventsApi();

  const body = {
    // string | Filter by contract ID (less preferred) (optional)
    contractId: contractId_example,
  } satisfies StreamEventsRequest;

  try {
    const data = await api.streamEvents(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **contractId** | `string` | Filter by contract ID (less preferred) | [Optional] [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | SSE stream of new events (text/event-stream) |  -  |
| **400** | Invalid contract_id format |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


## streamEventsByContract

> streamEventsByContract(contractId)

Stream new events for a specific contract in real time via Server-Sent Events.

### Example

```ts
import {
  Configuration,
  EventsApi,
} from '';
import type { StreamEventsByContractRequest } from '';

async function example() {
  console.log("🚀 Testing  SDK...");
  const api = new EventsApi();

  const body = {
    // string | Stellar contract ID
    contractId: contractId_example,
  } satisfies StreamEventsByContractRequest;

  try {
    const data = await api.streamEventsByContract(body);
    console.log(data);
  } catch (error) {
    console.error(error);
  }
}

// Run the test
example().catch(console.error);
```

### Parameters


| Name | Type | Description  | Notes |
|------------- | ------------- | ------------- | -------------|
| **contractId** | `string` | Stellar contract ID | [Defaults to `undefined`] |

### Return type

`void` (Empty response body)

### Authorization

No authorization required

### HTTP request headers

- **Content-Type**: Not defined
- **Accept**: Not defined


### HTTP response details
| Status code | Description | Response headers |
|-------------|-------------|------------------|
| **200** | SSE stream of contract events (text/event-stream) |  -  |
| **400** | Invalid contract_id format |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)

