# ContractSummary


## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**contract_id** | **str** |  | 
**event_count** | **int** |  | 
**latest_ledger** | **int** |  | 

## Example

```python
from openapi_client.models.contract_summary import ContractSummary

# TODO update the JSON string below
json = "{}"
# create an instance of ContractSummary from a JSON string
contract_summary_instance = ContractSummary.from_json(json)
# print the JSON string representation of the object
print(ContractSummary.to_json())

# convert the object into a dict
contract_summary_dict = contract_summary_instance.to_dict()
# create an instance of ContractSummary from a dict
contract_summary_from_dict = ContractSummary.from_dict(contract_summary_dict)
```
[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


