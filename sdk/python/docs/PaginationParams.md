# PaginationParams


## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**cursor** | **str** |  | [optional] 
**event_type** | [**EventType**](EventType.md) |  | [optional] 
**exact_count** | **bool** |  | [optional] 
**fields** | **str** |  | [optional] 
**from_ledger** | **int** |  | [optional] 
**limit** | **int** |  | [optional] 
**page** | **int** |  | [optional] 
**to_ledger** | **int** |  | [optional] 

## Example

```python
from openapi_client.models.pagination_params import PaginationParams

# TODO update the JSON string below
json = "{}"
# create an instance of PaginationParams from a JSON string
pagination_params_instance = PaginationParams.from_json(json)
# print the JSON string representation of the object
print(PaginationParams.to_json())

# convert the object into a dict
pagination_params_dict = pagination_params_instance.to_dict()
# create an instance of PaginationParams from a dict
pagination_params_from_dict = PaginationParams.from_dict(pagination_params_dict)
```
[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


