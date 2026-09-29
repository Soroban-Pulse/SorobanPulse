# Derived Metrics DSL

Derived metrics should be defined as named expressions over indexed event fields.

Example:

```toml
name = "failed_delivery_rate"
expression = "failed_deliveries / total_deliveries"
window = "5m"
```
