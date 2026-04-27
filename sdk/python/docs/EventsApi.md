# openapi_client.EventsApi

All URIs are relative to *http://localhost*

Method | HTTP request | Description
------------- | ------------- | -------------
[**get_contracts**](EventsApi.md#get_contracts) | **GET** /v1/contracts | 
[**get_events**](EventsApi.md#get_events) | **GET** /v1/events | 
[**get_events_by_contract**](EventsApi.md#get_events_by_contract) | **GET** /v1/events/contract/{contract_id} | 
[**get_events_by_tx**](EventsApi.md#get_events_by_tx) | **GET** /v1/events/tx/{tx_hash} | 
[**stream_events**](EventsApi.md#stream_events) | **GET** /v1/events/stream | Stream new events in real time via Server-Sent Events.
[**stream_events_by_contract**](EventsApi.md#stream_events_by_contract) | **GET** /v1/events/contract/{contract_id}/stream | Stream new events for a specific contract in real time via Server-Sent Events.


# **get_contracts**
> get_contracts(page=page, limit=limit)

### Example


```python
import openapi_client
from openapi_client.rest import ApiException
from pprint import pprint

# Defining the host is optional and defaults to http://localhost
# See configuration.py for a list of all supported configuration parameters.
configuration = openapi_client.Configuration(
    host = "http://localhost"
)


# Enter a context with an instance of the API client
async with openapi_client.ApiClient(configuration) as api_client:
    # Create an instance of the API class
    api_instance = openapi_client.EventsApi(api_client)
    page = 56 # int | Page number (default 1) (optional)
    limit = 56 # int | Items per page (1-100, default 20) (optional)

    try:
        await api_instance.get_contracts(page=page, limit=limit)
    except Exception as e:
        print("Exception when calling EventsApi->get_contracts: %s\n" % e)
```



### Parameters


Name | Type | Description  | Notes
------------- | ------------- | ------------- | -------------
 **page** | **int**| Page number (default 1) | [optional] 
 **limit** | **int**| Items per page (1-100, default 20) | [optional] 

### Return type

void (empty response body)

### Authorization

No authorization required

### HTTP request headers

 - **Content-Type**: Not defined
 - **Accept**: Not defined

### HTTP response details

| Status code | Description | Response headers |
|-------------|-------------|------------------|
**200** | Paginated list of indexed contract IDs |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

# **get_events**
> get_events(page=page, limit=limit, exact_count=exact_count, event_type=event_type, from_ledger=from_ledger, to_ledger=to_ledger)

### Example


```python
import openapi_client
from openapi_client.rest import ApiException
from pprint import pprint

# Defining the host is optional and defaults to http://localhost
# See configuration.py for a list of all supported configuration parameters.
configuration = openapi_client.Configuration(
    host = "http://localhost"
)


# Enter a context with an instance of the API client
async with openapi_client.ApiClient(configuration) as api_client:
    # Create an instance of the API class
    api_instance = openapi_client.EventsApi(api_client)
    page = 56 # int | Page number (default: 1) (optional)
    limit = 56 # int | Results per page, 1–100 (default: 20) (optional)
    exact_count = True # bool | Use exact COUNT(*) instead of approximate (optional)
    event_type = openapi_client.EventType() # EventType | Filter by event type: contract, diagnostic, system (optional)
    from_ledger = 56 # int | Return events at or after this ledger (optional)
    to_ledger = 56 # int | Return events at or before this ledger (optional)

    try:
        await api_instance.get_events(page=page, limit=limit, exact_count=exact_count, event_type=event_type, from_ledger=from_ledger, to_ledger=to_ledger)
    except Exception as e:
        print("Exception when calling EventsApi->get_events: %s\n" % e)
```



### Parameters


Name | Type | Description  | Notes
------------- | ------------- | ------------- | -------------
 **page** | **int**| Page number (default: 1) | [optional] 
 **limit** | **int**| Results per page, 1–100 (default: 20) | [optional] 
 **exact_count** | **bool**| Use exact COUNT(*) instead of approximate | [optional] 
 **event_type** | [**EventType**](.md)| Filter by event type: contract, diagnostic, system | [optional] 
 **from_ledger** | **int**| Return events at or after this ledger | [optional] 
 **to_ledger** | **int**| Return events at or before this ledger | [optional] 

### Return type

void (empty response body)

### Authorization

No authorization required

### HTTP request headers

 - **Content-Type**: Not defined
 - **Accept**: Not defined

### HTTP response details

| Status code | Description | Response headers |
|-------------|-------------|------------------|
**200** | Paginated list of events |  -  |
**400** | Invalid query parameters |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

# **get_events_by_contract**
> get_events_by_contract(contract_id, page=page, limit=limit, from_ledger=from_ledger, to_ledger=to_ledger)

### Example


```python
import openapi_client
from openapi_client.rest import ApiException
from pprint import pprint

# Defining the host is optional and defaults to http://localhost
# See configuration.py for a list of all supported configuration parameters.
configuration = openapi_client.Configuration(
    host = "http://localhost"
)


# Enter a context with an instance of the API client
async with openapi_client.ApiClient(configuration) as api_client:
    # Create an instance of the API class
    api_instance = openapi_client.EventsApi(api_client)
    contract_id = 'contract_id_example' # str | Stellar contract ID (56-char, starts with C)
    page = 56 # int | Page number (default: 1) (optional)
    limit = 56 # int | Results per page, 1–100 (default: 20) (optional)
    from_ledger = 56 # int | Return events at or after this ledger (optional)
    to_ledger = 56 # int | Return events at or before this ledger (optional)

    try:
        await api_instance.get_events_by_contract(contract_id, page=page, limit=limit, from_ledger=from_ledger, to_ledger=to_ledger)
    except Exception as e:
        print("Exception when calling EventsApi->get_events_by_contract: %s\n" % e)
```



### Parameters


Name | Type | Description  | Notes
------------- | ------------- | ------------- | -------------
 **contract_id** | **str**| Stellar contract ID (56-char, starts with C) | 
 **page** | **int**| Page number (default: 1) | [optional] 
 **limit** | **int**| Results per page, 1–100 (default: 20) | [optional] 
 **from_ledger** | **int**| Return events at or after this ledger | [optional] 
 **to_ledger** | **int**| Return events at or before this ledger | [optional] 

### Return type

void (empty response body)

### Authorization

No authorization required

### HTTP request headers

 - **Content-Type**: Not defined
 - **Accept**: Not defined

### HTTP response details

| Status code | Description | Response headers |
|-------------|-------------|------------------|
**200** | Events for the given contract |  -  |
**400** | Invalid contract_id format or ledger range |  -  |
**404** | No events found for contract |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

# **get_events_by_tx**
> get_events_by_tx(tx_hash)

### Example


```python
import openapi_client
from openapi_client.rest import ApiException
from pprint import pprint

# Defining the host is optional and defaults to http://localhost
# See configuration.py for a list of all supported configuration parameters.
configuration = openapi_client.Configuration(
    host = "http://localhost"
)


# Enter a context with an instance of the API client
async with openapi_client.ApiClient(configuration) as api_client:
    # Create an instance of the API class
    api_instance = openapi_client.EventsApi(api_client)
    tx_hash = 'tx_hash_example' # str | Transaction hash (64 lowercase hex chars)

    try:
        await api_instance.get_events_by_tx(tx_hash)
    except Exception as e:
        print("Exception when calling EventsApi->get_events_by_tx: %s\n" % e)
```



### Parameters


Name | Type | Description  | Notes
------------- | ------------- | ------------- | -------------
 **tx_hash** | **str**| Transaction hash (64 lowercase hex chars) | 

### Return type

void (empty response body)

### Authorization

No authorization required

### HTTP request headers

 - **Content-Type**: Not defined
 - **Accept**: Not defined

### HTTP response details

| Status code | Description | Response headers |
|-------------|-------------|------------------|
**200** | Events for the given transaction (empty array if none) |  -  |
**400** | Invalid tx_hash format |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

# **stream_events**
> stream_events(contract_id=contract_id)

Stream new events in real time via Server-Sent Events.

This endpoint is less preferred for contract-specific streaming; use
`/v1/events/contract/{contract_id}/stream` instead.

### Example


```python
import openapi_client
from openapi_client.rest import ApiException
from pprint import pprint

# Defining the host is optional and defaults to http://localhost
# See configuration.py for a list of all supported configuration parameters.
configuration = openapi_client.Configuration(
    host = "http://localhost"
)


# Enter a context with an instance of the API client
async with openapi_client.ApiClient(configuration) as api_client:
    # Create an instance of the API class
    api_instance = openapi_client.EventsApi(api_client)
    contract_id = 'contract_id_example' # str | Filter by contract ID (less preferred) (optional)

    try:
        # Stream new events in real time via Server-Sent Events.
        await api_instance.stream_events(contract_id=contract_id)
    except Exception as e:
        print("Exception when calling EventsApi->stream_events: %s\n" % e)
```



### Parameters


Name | Type | Description  | Notes
------------- | ------------- | ------------- | -------------
 **contract_id** | **str**| Filter by contract ID (less preferred) | [optional] 

### Return type

void (empty response body)

### Authorization

No authorization required

### HTTP request headers

 - **Content-Type**: Not defined
 - **Accept**: Not defined

### HTTP response details

| Status code | Description | Response headers |
|-------------|-------------|------------------|
**200** | SSE stream of new events (text/event-stream) |  -  |
**400** | Invalid contract_id format |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

# **stream_events_by_contract**
> stream_events_by_contract(contract_id)

Stream new events for a specific contract in real time via Server-Sent Events.

### Example


```python
import openapi_client
from openapi_client.rest import ApiException
from pprint import pprint

# Defining the host is optional and defaults to http://localhost
# See configuration.py for a list of all supported configuration parameters.
configuration = openapi_client.Configuration(
    host = "http://localhost"
)


# Enter a context with an instance of the API client
async with openapi_client.ApiClient(configuration) as api_client:
    # Create an instance of the API class
    api_instance = openapi_client.EventsApi(api_client)
    contract_id = 'contract_id_example' # str | Stellar contract ID

    try:
        # Stream new events for a specific contract in real time via Server-Sent Events.
        await api_instance.stream_events_by_contract(contract_id)
    except Exception as e:
        print("Exception when calling EventsApi->stream_events_by_contract: %s\n" % e)
```



### Parameters


Name | Type | Description  | Notes
------------- | ------------- | ------------- | -------------
 **contract_id** | **str**| Stellar contract ID | 

### Return type

void (empty response body)

### Authorization

No authorization required

### HTTP request headers

 - **Content-Type**: Not defined
 - **Accept**: Not defined

### HTTP response details

| Status code | Description | Response headers |
|-------------|-------------|------------------|
**200** | SSE stream of contract events (text/event-stream) |  -  |
**400** | Invalid contract_id format |  -  |

[[Back to top]](#) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to Model list]](../README.md#documentation-for-models) [[Back to README]](../README.md)

