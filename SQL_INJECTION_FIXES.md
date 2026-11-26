# Issue #1448 - SQL Injection Prevention via Bound Parameters

## Problem
Using `format!()` for SQL building makes SQL injection vulnerable and hard to audit.

## Fixes Required

### 1. src/db.rs:31 - statement_timeout
❌ **Before:**
```rust
sqlx::query(&format!("SET LOCAL statement_timeout = '{timeout_ms}ms'"))
```

✅ **After:**
```rust
// Use prepared statement - timeout_ms is numeric, safe to format for display only
let query = format!("SET LOCAL statement_timeout = {}", timeout_ms);
sqlx::query(&query)
```

Or better:
```rust
// Execute via connection parameter setting (Postgres-specific)
conn.execute(format!("SET LOCAL statement_timeout = {}", timeout_ms).as_str())
```

### 2. src/db_pool/mod.rs:91 - connection pool timeout
❌ **Before:**
```rust
conn.execute(format!("SET statement_timeout = '{statement_timeout_ms}ms'").as_str())
```

✅ **After:**
```rust
conn.execute(format!("SET statement_timeout = {}", statement_timeout_ms).as_str())
```

### 3. src/stats_refresh.rs:53 - Materialized view refresh
❌ **Before:**
```rust
format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {view}")
```

✅ **After:**
```rust
// Use identifier escaping for view names (user-controlled potentially)
let view_name = format!("\"{}\"", view.replace("\"", "\"\""));
format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {}", view_name)
```

Or use sqlx identifier helpers:
```rust
sqlx::query(&format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {}", 
  escape_identifier(&view)))
```

### 4. src/partition_manager.rs - Dynamic partition names
❌ **Before (line 234-236):**
```rust
conn.execute(format!("CREATE TABLE IF NOT EXISTS events_{} ...", partition_id).as_str())
```

✅ **After:**
```rust
// Validate partition_id format (YYYY_MM) before interpolation
let partition_id_validated = validate_partition_id(&partition_id)?;
let table_name = format!("events_{}", partition_id_validated);
let query = format!("CREATE TABLE IF NOT EXISTS \"{}\" ...", 
  table_name.replace("\"", "\"\""));
conn.execute(query.as_str())
```

## Validation Strategy

Create `src/sql_helpers.rs`:

```rust
/// Validate partition ID format (YYYY_MM)
pub fn validate_partition_id(id: &str) -> Result<String, Error> {
    if id.len() != 7 || id.chars().nth(4) != Some('_') {
        return Err(Error::InvalidPartitionId);
    }
    
    let year: u16 = id[0..4].parse()?;
    let month: u8 = id[5..7].parse()?;
    
    if year < 2000 || year > 2100 || month < 1 || month > 12 {
        return Err(Error::InvalidPartitionId);
    }
    
    Ok(id.to_string())
}

/// Escape SQL identifier (double quotes, escape internal quotes)
pub fn escape_identifier(id: &str) -> String {
    format!("\"{}\"", id.replace("\"", "\"\""))
}

/// Validate and escape view name
pub fn escape_view_name(view: &str) -> Result<String, Error> {
    // Only allow alphanumeric, underscore, and dot
    if !view.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '.') {
        return Err(Error::InvalidIdentifier);
    }
    Ok(escape_identifier(view))
}
```

## Audit Checklist

- [ ] Review all `format!("SELECT ...)` → ensure no user input
- [ ] Review all `format!("CREATE ...)` → validate identifiers
- [ ] Review all `format!("SET ...)` → ensure no user input
- [ ] Add sql_helpers.rs module with validation functions
- [ ] Update partition_manager.rs to use validate_partition_id()
- [ ] Update stats_refresh.rs to use escape_view_name()
- [ ] Update db.rs and db_pool/mod.rs for timeout values
- [ ] Add tests: test_validate_partition_id(), test_escape_identifier()
- [ ] CI: Add clippy lint for `format!` in SQL context

## Prevention Pattern

Going forward:
```rust
// SAFE: Only static identifiers
sqlx::query("SELECT * FROM events WHERE tenant_id = $1")
    .bind(tenant_id)
    .fetch_all(pool)

// SAFE: Validated dynamic identifiers
let table = validate_partition_id(&partition_id)?;
let query = format!("SELECT * FROM events_{}", table);
sqlx::query(&query).bind(tenant_id).fetch_all(pool)

// UNSAFE: Never do this
let view = get_user_input();
sqlx::query(&format!("SELECT * FROM {}", view)) // ❌ SQL injection!
```
