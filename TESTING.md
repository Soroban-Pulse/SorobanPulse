# Database Testing Setup

This document explains how to set up and run the database tests for Soroban Pulse.

## Prerequisites

1. PostgreSQL installed and running
2. Rust development environment
3. Required system dependencies:
   ```bash
   # Ubuntu/Debian
   sudo apt update
   sudo apt install -y libpq-dev pkg-config libssl-dev
   
   # macOS
   brew install postgresql openssl pkg-config
   
   # Other systems may require different packages
   ```

## Test Database Setup

The tests use `sqlx::test` which automatically creates and manages test databases. You need to configure PostgreSQL to allow this:

### Option 1: Using DATABASE_URL (Recommended)

Set the `DATABASE_URL` environment variable to point to your PostgreSQL instance:

```bash
export DATABASE_URL="postgres://username:password@localhost/soroban_pulse_test"
```

### Option 2: Using .env file

Create a `.env` file in the project root:

```
DATABASE_URL="postgres://username:password@localhost/soroban_pulse_test"
```

## Running Tests

### All Tests
```bash
cargo test
```

### Specific Test Modules
```bash
# Only handler tests
cargo test handlers::tests

# Only pagination tests
cargo test pagination

# Only specific test
cargo test health_happy_path_returns_200
```

### Test with Output
```bash
cargo test -- --nocapture
```

## Test Database Permissions

The database user needs:
- CREATE DATABASE permissions (for test database creation)
- Schema modification permissions (for migrations)

## CI/CD Setup

For CI/CD, ensure:
1. PostgreSQL service is running
2. `DATABASE_URL` is set as an environment variable
3. Required system packages are installed

Example GitHub Actions setup:
```yaml
services:
  postgres:
    image: postgres:15
    env:
      POSTGRES_PASSWORD: postgres
      POSTGRES_DB: soroban_pulse_test
    options: >-
      --health-cmd pg_isready
      --health-interval 10s
      --health-timeout 5s
      --health-retries 5

env:
  DATABASE_URL: postgres://postgres:postgres@localhost/soroban_pulse_test
```

## Test Coverage

The test suite covers:

### Handler Tests
- ✅ Health endpoint (happy path, DB unreachable)
- ✅ Status endpoint (operational info)
- ✅ OpenAPI spec endpoint
- ✅ Main events endpoint (empty, with data, validation errors)
- ✅ Events by contract (happy path, not found, validation)
- ✅ Events by transaction (happy path, validation)
- ✅ Stream events (SSE format, with filters)
- ✅ Metrics endpoint (Prometheus format)

### Pagination Tests
- ✅ Boundary conditions (min/max limits, page edges)
- ✅ Parameter validation and clamping
- ✅ Exact vs approximate counting
- ✅ Filtering with pagination
- ✅ Field selection with pagination

### Error Handling Tests
- ✅ Invalid input validation
- ✅ Database error handling
- ✅ Format validation (contract IDs, transaction hashes)
- ✅ Response format verification

All tests use real PostgreSQL instances via `sqlx::test` with automatic database creation and migration.
