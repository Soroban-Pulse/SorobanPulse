
# Event


## Properties

Name | Type
------------ | -------------
`contractId` | string
`createdAt` | Date
`eventData` | any
`eventType` | [EventType](EventType.md)
`id` | string
`ledger` | number
`timestamp` | Date
`txHash` | string

## Example

```typescript
import type { Event } from ''

// TODO: Update the object below with actual values
const example = {
  "contractId": null,
  "createdAt": null,
  "eventData": null,
  "eventType": null,
  "id": null,
  "ledger": null,
  "timestamp": null,
  "txHash": null,
} satisfies Event

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as Event
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


