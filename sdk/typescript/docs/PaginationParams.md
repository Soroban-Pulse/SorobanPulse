
# PaginationParams


## Properties

Name | Type
------------ | -------------
`cursor` | string
`eventType` | [EventType](EventType.md)
`exactCount` | boolean
`fields` | string
`fromLedger` | number
`limit` | number
`page` | number
`toLedger` | number

## Example

```typescript
import type { PaginationParams } from ''

// TODO: Update the object below with actual values
const example = {
  "cursor": null,
  "eventType": null,
  "exactCount": null,
  "fields": null,
  "fromLedger": null,
  "limit": null,
  "page": null,
  "toLedger": null,
} satisfies PaginationParams

console.log(example)

// Convert the instance to a JSON string
const exampleJSON: string = JSON.stringify(example)
console.log(exampleJSON)

// Parse the JSON string back to an object
const exampleParsed = JSON.parse(exampleJSON) as PaginationParams
console.log(exampleParsed)
```

[[Back to top]](#) [[Back to API list]](../README.md#api-endpoints) [[Back to Model list]](../README.md#models) [[Back to README]](../README.md)


