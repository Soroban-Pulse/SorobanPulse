# Event Filtering DSL

## Overview

The Event Filtering DSL (Issue #928) provides a composable, JSON-encoded
expression language for filtering events beyond the capabilities of simple
query-string parameters.

Use it when you need:
- Compound predicates that mix `AND`, `OR`, and `NOT` logic.
- Range queries such as `ledger BETWEEN 5000000 AND 6000000`.
- Membership tests: `event_type IN ("contract", "diagnostic")`.
- Substring matching on string fields.
- Named, reusable filters stored server-side.

Expressions are submitted as JSON, validated against an allow-list of
fields and operators, transpiled to a parameterised SQL `WHERE` fragment,
and then executed against the `events` table.

---

## Endpoints

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| `POST` | `/v1/events/filter` | optional | Execute a DSL filter, return events |
| `POST` | `/v1/admin/dsl/compile` | admin | Compile & validate DSL, return SQL |
| `POST` | `/v1/admin/dsl/filters` | admin | Persist a named filter |
| `GET`  | `/v1/admin/dsl/filters` | admin | List saved filters |

---

## FilterExpr Grammar

### EBNF

```ebnf
expr        ::= and_expr | or_expr | not_expr | eq_expr | gt_expr | lt_expr
              | contains_expr | in_expr | between_expr | exists_expr

and_expr    ::= '{"type":"and","filters":[' expr (',' expr)* ']}'
or_expr     ::= '{"type":"or","filters":[' expr (',' expr)* ']}'
not_expr    ::= '{"type":"not","filter":' expr '}'

eq_expr     ::= '{"type":"eq","field":"' field '","value":' json_value '}'
gt_expr     ::= '{"type":"gt","field":"' field '","value":' json_value '}'
lt_expr     ::= '{"type":"lt","field":"' field '","value":' json_value '}'
contains_expr ::= '{"type":"contains","field":"' field '","value":"' string '"}'
in_expr     ::= '{"type":"in","field":"' field '","values":[' json_value (',' json_value)* ']}'
between_expr ::= '{"type":"between","field":"' field '","min":' json_value ',"max":' json_value '}'
exists_expr ::= '{"type":"exists","field":"' field '"}'

field       ::= "contract_id" | "event_type" | "ledger" | "timestamp"
              | "tx_hash" | "schema_version" | "in_successful_call" | "tenant_id"
json_value  ::= string | number | boolean | null
```

### Maximum depth

Expression trees are limited to **10 levels of nesting**.  Deeper trees are
rejected with a `422 Unprocessable Entity` error.

---

## Supported Fields

| Field | SQL column | Types |
|-------|-----------|-------|
| `contract_id` | `contract_id` | string |
| `event_type` | `event_type::text` | `"contract"`, `"diagnostic"`, `"system"` |
| `ledger` | `ledger` | integer |
| `timestamp` | `timestamp` | ISO-8601 string or integer epoch |
| `tx_hash` | `tx_hash` | string |
| `schema_version` | `schema_version` | integer |
| `in_successful_call` | `in_successful_call` | boolean |
| `tenant_id` | `tenant_id` | string |

Any field not in this list is rejected by the validator.

---

## Supported Operators

| Operator | JSON tag | SQL equivalent |
|----------|----------|----------------|
| Equality | `eq` | `field = $n` |
| Greater-than | `gt` | `field > $n` |
| Less-than | `lt` | `field < $n` |
| Substring | `contains` | `field ILIKE '%value%'` |
| Membership | `in` | `field IN ($1, $2, …)` |
| Range | `between` | `field BETWEEN $n AND $m` |
| Existence | `exists` | `field IS NOT NULL` |
| Logical AND | `and` | `(… AND …)` |
| Logical OR | `or` | `(… OR …)` |
| Logical NOT | `not` | `NOT …` |

---

## Examples

### Simple equality

```json
{
  "type": "eq",
  "field": "contract_id",
  "value": "CABC1234DEFG5678"
}
```

SQL: `contract_id = $1`

---

### Compound AND filter

```json
{
  "type": "and",
  "filters": [
    { "type": "eq",      "field": "event_type", "value": "contract" },
    { "type": "between", "field": "ledger",      "min": 5000000, "max": 6000000 }
  ]
}
```

SQL: `(event_type::text = $1 AND ledger BETWEEN $2 AND $3)`

---

### Nested NOT / OR

```json
{
  "type": "not",
  "filter": {
    "type": "or",
    "filters": [
      { "type": "eq", "field": "event_type", "value": "diagnostic" },
      { "type": "eq", "field": "event_type", "value": "system" }
    ]
  }
}
```

SQL: `NOT (event_type::text = $1 OR event_type::text = $2)`

---

### Complex multi-contract range filter

```json
{
  "type": "and",
  "filters": [
    {
      "type": "in",
      "field": "contract_id",
      "values": ["CA...", "CB...", "CC..."]
    },
    { "type": "gt", "field": "ledger", "value": 4000000 },
    { "type": "eq", "field": "in_successful_call", "value": true }
  ]
}
```

SQL: `(contract_id IN ($1, $2, $3) AND ledger > $4 AND in_successful_call = $5)`

---

## SQL Transpilation Examples

| DSL expr | Transpiled SQL |
|----------|----------------|
| `Eq { contract_id, "CABC" }` | `contract_id = $1` |
| `Between { ledger, 100, 200 }` | `ledger BETWEEN $1 AND $2` |
| `Contains { tx_hash, "abc" }` | `tx_hash ILIKE $1` (`%abc%`) |
| `In { event_type, ["contract","system"] }` | `event_type::text IN ($1, $2)` |
| `Exists { tenant_id }` | `tenant_id IS NOT NULL` |
| `Not { Exists { tenant_id } }` | `NOT tenant_id IS NOT NULL` |

---

## Optimization Rules

The server automatically optimises DSL trees before transpilation:

| Rule | Before | After |
|------|--------|-------|
| AND flattening | `And([And([A, B]), C])` | `And([A, B, C])` |
| OR flattening | `Or([Or([A, B]), C])` | `Or([A, B, C])` |
| Double-NOT elimination | `Not(Not(E))` | `E` |
| Single-child unwrap | `And([E])` | `E` |
| Single-child unwrap | `Or([E])` | `E` |

---

## Admin Endpoint Usage

### Compile and validate without executing

```bash
curl -X POST /v1/admin/dsl/compile \
  -H "Authorization: Bearer $ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "expression": { "type": "eq", "field": "contract_id", "value": "CABC" },
    "description": "single contract filter"
  }'
```

Response:

```json
{
  "sql": "contract_id = $1",
  "params": ["CABC"],
  "param_count": 1
}
```

---

### Save a named filter

```bash
curl -X POST /v1/admin/dsl/filters \
  -H "Authorization: Bearer $ADMIN_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "high-ledger-contract-events",
    "filter": {
      "expression": {
        "type": "and",
        "filters": [
          { "type": "eq", "field": "event_type", "value": "contract" },
          { "type": "gt", "field": "ledger", "value": 5000000 }
        ]
      },
      "description": "Contract events after ledger 5M"
    }
  }'
```

---

### List saved filters

```bash
curl /v1/admin/dsl/filters \
  -H "Authorization: Bearer $ADMIN_API_KEY"
```

---

### Execute a DSL filter

```bash
curl -X POST /v1/events/filter \
  -H "Content-Type: application/json" \
  -d '{
    "filter": {
      "type": "between",
      "field": "ledger",
      "min": 5000000,
      "max": 6000000
    },
    "page": 1,
    "limit": 50
  }'
```

---

## Depth and Size Limits

| Limit | Value |
|-------|-------|
| Maximum expression depth | 10 |
| Maximum `In` values | 100 (recommended, not enforced) |
| Maximum `page` size | 500 |

---

## Security Considerations

- **Field injection prevention**: Only fields in the allow-list are accepted.
  Unknown field names cause a `422` error before any SQL is generated.
- **Parameterisation**: All values are bound as SQL parameters; string
  concatenation is never used in the transpiler.
- **Depth limit**: Pathological trees (very deep recursion) are rejected at
  validation time.
- **Admin gating**: The `/compile` and `/filters` admin endpoints require
  `ADMIN_API_KEY`.  The `/events/filter` execution endpoint respects the
  regular `API_KEY` gate.

---

## Error Reference

| Code | HTTP Status | Meaning |
|------|-------------|---------|
| `VALIDATION_ERROR` | 422 | One or more DSL validation errors |
| `DSL_PARSE_ERROR` | 422 | JSON body could not be parsed as a DSL expression |
| `DSL_TOO_DEEP` | 422 | Expression exceeds maximum nesting depth |
| `DSL_EMPTY` | 422 | An `And`/`Or` node has no children |
| `UNAUTHORIZED` | 401 | Missing authentication credential |
| `FORBIDDEN` | 403 | Admin endpoint requires `ADMIN_API_KEY` |
| `INTERNAL_ERROR` | 500 | Database failure during query execution |
