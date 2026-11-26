# Issue #1475 - Consolidate JavaScript/TypeScript SDKs

## Problem

Two competing JS/TS SDKs with different npm scopes; users confused about which is official.

## Current State

```
sdk/javascript/
├── package.json → "@sorobanpulse/sdk"
├── src/
├── tests/
└── examples/

sdk/typescript/
├── package.json → "@soroban-pulse/sdk"
├── apis/           (OpenAPI-generated)
├── models/         (OpenAPI-generated)
├── src/
│   ├── sse.ts
│   ├── retry-policy.ts
│   ├── interceptors.ts
│   └── webhook-verification.ts
├── tsup.config.ts
└── ...

Publishing:
  - sdk-npm-publish.yml (unclear which package)
  - sdk-codegen.yml (TypeScript only)

Dependencies:
  - packages/feed-widget
  - vscode-extension
  → Can depend on either!

npm Scopes:
  - @sorobanpulse (typo? unofficial?)
  - @soroban-pulse (official?)
```

## Solution

### Phase 1 - Audit & Decision (PR 1)

**Investigation steps:**

```bash
# Check which npm scope org actually owns
npm org set @soroban-pulse    # This should succeed
npm org set @sorobanpulse      # This might fail

# Check package popularity
npm view @sorobanpulse/sdk
npm view @soroban-pulse/sdk

# Check which SDKs depend on which
grep -r "@sorobanpulse/sdk" . --include="*.json" --include="*.ts" --include="*.js"
grep -r "@soroban-pulse/sdk" . --include="*.json" --include="*.ts" --include="*.js"
```

**Recommendation:**
- **Official scope:** `@soroban-pulse` (matches org name, likely owns it)
- **Official directory:** `sdk/typescript/` (has hand-written helpers, more complete)
- **Deprecate:** `sdk/javascript/` or merge into `sdk/typescript/`

### Phase 2 - Merge Hand-Written Helpers (PR 2)

**Merge sdk/typescript/src/* into canonical location:**

```bash
# Copy hand-written helpers to sdk/typescript/src/
cp sdk/javascript/src/interceptors.ts sdk/typescript/src/
cp sdk/javascript/src/sse.ts sdk/typescript/src/
cp sdk/javascript/src/retry-policy.ts sdk/typescript/src/
cp sdk/javascript/src/webhook-verification.ts sdk/typescript/src/

# Create unified index.ts
cat > sdk/typescript/src/index.ts << 'EOF'
// Generated client (from OpenAPI)
export * from "./_generated";

// Hand-written helpers
export { RetryPolicy, exponentialBackoff } from "./retry-policy";
export { SSEEventSource, SSEEventListener } from "./sse";
export {
  HttpInterceptor,
  RetryInterceptor,
  LoggingInterceptor,
  AuthInterceptor,
} from "./interceptors";
export { verifyWebhookSignature, WebhookSignatureError } from "./webhook-verification";

// High-level client wrapper
export { SorobanPulseClient } from "./client";
export type { ClientConfig } from "./client";

export const VERSION = "0.1.0";
EOF
```

**Create sdk/typescript/src/client.ts (High-level wrapper):**

```typescript
import {
  Configuration,
  DefaultApi,
  HttpInterceptor,
  RetryPolicy,
  SSEEventSource,
} from "./_generated";
import { RetryInterceptor, LoggingInterceptor } from "./interceptors";

export interface ClientConfig {
  baseUrl: string;
  apiKey?: string;
  retryPolicy?: RetryPolicy;
  interceptors?: HttpInterceptor[];
  timeout?: number;
}

export class SorobanPulseClient {
  private api: DefaultApi;
  private sse: SSEEventSource;

  constructor(config: ClientConfig) {
    const interceptors = [
      new LoggingInterceptor(),
      new RetryInterceptor(config.retryPolicy),
      ...(config.interceptors || []),
    ];

    const apiConfig = new Configuration({
      basePath: config.baseUrl,
      apiKey: config.apiKey,
      interceptors,
    });

    this.api = new DefaultApi(apiConfig);
    this.sse = new SSEEventSource(config.baseUrl);
  }

  // Forward API methods
  async getTransaction(id: string) {
    return this.api.getTransaction(id);
  }

  async listTransactions(limit?: number, offset?: number) {
    return this.api.listTransactions(limit, offset);
  }

  // SSE streaming
  subscribeToTransactions(callback: (tx: any) => void) {
    return this.sse.subscribe("transactions", callback);
  }

  subscribeToContract(contractId: string, callback: (event: any) => void) {
    return this.sse.subscribe(`contracts/${contractId}`, callback);
  }
}
```

### Phase 3 - Deprecate Old Package (PR 3)

**Publish deprecation notice:**

```bash
# Mark old package as deprecated
npm deprecate @sorobanpulse/sdk "DEPRECATED: Use @soroban-pulse/sdk instead"

# Keep the package but add deprecation notice to source
cat > sdk/javascript/README.md << 'EOF'
# ⚠️ DEPRECATED

This package (@sorobanpulse/sdk) is deprecated.

**Use [@soroban-pulse/sdk](https://www.npmjs.com/package/@soroban-pulse/sdk) instead.**

This was a naming error. The official SDK is published under @soroban-pulse:

```bash
npm install @soroban-pulse/sdk
```

## Migration Guide

```typescript
// OLD (deprecated)
import { SorobanClient } from "@sorobanpulse/sdk";

// NEW (official)
import { SorobanPulseClient } from "@soroban-pulse/sdk";
```

If you're using this package, please migrate to @soroban-pulse/sdk.
EOF
```

### Phase 4 - Update All Dependencies (PR 4)

**Find and update all imports:**

```bash
# Update package.json files
sed -i 's/"@sorobanpulse\/sdk"/"@soroban-pulse\/sdk"/g' sdk/*/package.json
sed -i 's/"@sorobanpulse\/sdk"/"@soroban-pulse\/sdk"/g' packages/*/package.json
sed -i 's/"@sorobanpulse\/sdk"/"@soroban-pulse\/sdk"/g' vscode-extension/package.json

# Update TypeScript imports
find . -name "*.ts" -o -name "*.tsx" | xargs sed -i \
  's/from "@sorobanpulse\/sdk/from "@soroban-pulse\/sdk/g'
find . -name "*.ts" -o -name "*.tsx" | xargs sed -i \
  's/import "@sorobanpulse\/sdk/import "@soroban-pulse\/sdk/g'

# Update documentation
sed -i 's/@sorobanpulse\/sdk/@soroban-pulse\/sdk/g' docs/client-libraries.md
sed -i 's/@sorobanpulse\/sdk/@soroban-pulse\/sdk/g' README.md
sed -i 's/@sorobanpulse\/sdk/@soroban-pulse\/sdk/g' docs/**/*.md
```

**Updated docs/client-libraries.md:**

```markdown
# Client Libraries

## Official SDKs

All SDKs follow the same API patterns and are version-synchronized.

### TypeScript / JavaScript

**Official package:** `@soroban-pulse/sdk`

```bash
npm install @soroban-pulse/sdk
```

**Features:**
- Full OpenAPI client with generated types
- Retry policy with exponential backoff
- Server-Sent Events (SSE) streaming
- Webhook signature verification
- Request/response interceptors
- TypeScript support with full types

**Example:**

```typescript
import { SorobanPulseClient } from "@soroban-pulse/sdk";

const client = new SorobanPulseClient({
  baseUrl: "https://api.sorobanpulse.com",
  apiKey: process.env.API_KEY,
});

// REST API
const tx = await client.getTransaction("tx-id");

// Real-time streaming
client.subscribeToTransactions((tx) => {
  console.log("New transaction:", tx);
});
```

**Note:** The package `@sorobanpulse/sdk` is deprecated. Please use `@soroban-pulse/sdk`.

### Python

**Official package:** `soroban-pulse`

```bash
pip install soroban-pulse
```

### Go

**Official module:** `github.com/soroban-pulse/client-go`

```bash
go get github.com/soroban-pulse/client-go
```
```

### Phase 5 - Update Publishing Workflow (PR 5)

**Consolidate .github/workflows/sdk-npm-publish.yml:**

```yaml
name: Publish TypeScript SDK

on:
  push:
    tags:
      - "v*"

jobs:
  publish:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: "18"
      - name: Install dependencies
        run: |
          cd sdk/typescript
          npm install
      - name: Run tests
        run: |
          cd sdk/typescript
          npm test
      - name: Build
        run: |
          cd sdk/typescript
          npm run build
      - name: Publish to npm
        run: |
          cd sdk/typescript
          npm set "//registry.npmjs.org/:_authToken=${{ secrets.NPM_TOKEN }}"
          npm publish --access public
      - name: Publish deprecation for old package
        run: |
          npm set "//registry.npmjs.org/:_authToken=${{ secrets.NPM_TOKEN }}"
          npm deprecate @sorobanpulse/sdk "DEPRECATED: Use @soroban-pulse/sdk instead" || true
```

**Delete .github/workflows/sdk-codegen.yml** (no longer needed; codegen integrated into main build)

### Phase 6 - Reserve Namespace (PR 6)

**Create stub package for @sorobanpulse/sdk (typo scope):**

```bash
# This prevents someone from typosquatting the typo
mkdir -p npm-namespace-reserve/typo-reservation
cat > npm-namespace-reserve/typo-reservation/package.json << 'EOF'
{
  "name": "@sorobanpulse/sdk",
  "version": "1.0.0",
  "description": "DEPRECATED: This was a typo. Use @soroban-pulse/sdk instead.",
  "deprecated": true,
  "repository": "https://github.com/soroban-pulse/soroban-pulse",
  "keywords": ["deprecated", "typo"],
  "homepage": "https://www.npmjs.com/package/@soroban-pulse/sdk"
}
EOF

# Publish to npm (optional: publish empty stub to prevent typosquatting)
npm publish npm-namespace-reserve/typo-reservation/
```

### Phase 7 - Documentation Update (PR 7)

**Create MIGRATION.md:**

```markdown
# SDK Migration Guide

## JavaScript/TypeScript SDK Consolidation

The JavaScript SDK (@sorobanpulse/sdk) and TypeScript SDK (@soroban-pulse/sdk) have been consolidated.

### What Changed

- **Official SDK:** `@soroban-pulse/sdk` (TypeScript)
- **Deprecated:** `@sorobanpulse/sdk` (old, typo'd scope)

### Migration Steps

#### 1. Update package.json

```bash
npm uninstall @sorobanpulse/sdk
npm install @soroban-pulse/sdk
```

#### 2. Update imports

```typescript
// OLD
import { SorobanClient } from "@sorobanpulse/sdk";
const client = new SorobanClient();

// NEW
import { SorobanPulseClient } from "@soroban-pulse/sdk";
const client = new SorobanPulseClient({
  baseUrl: "https://api.sorobanpulse.com",
  apiKey: process.env.API_KEY,
});
```

#### 3. API Differences

The new client has a cleaner API:

```typescript
// Old SDK
client.getTransaction(id);
client.createTransaction(tx);

// New SDK (unified through SorobanPulseClient)
const client = new SorobanPulseClient(config);
client.getTransaction(id);    // Same method names
client.createTransaction(tx);
```

All methods remain the same; only the class name and initialization changed.

### Version Alignment

As of version 0.1.0, all SDKs (TS, Python, Go) are version-synchronized:

- TypeScript/JavaScript: 0.1.0+
- Python: 0.1.0+
- Go: 0.1.0+

You can safely mix SDKs from the same release.
```

### Checklist

- [ ] Audit npm scopes and package popularity
- [ ] Decide official package/directory
- [ ] Copy hand-written helpers to canonical SDK
- [ ] Create unified index.ts and high-level client wrapper
- [ ] Deprecate old package on npm
- [ ] Update all dependencies across repo
- [ ] Update docs/client-libraries.md
- [ ] Update CI workflows (consolidate)
- [ ] Delete redundant workflow files
- [ ] Reserve typo namespace (optional)
- [ ] Create MIGRATION.md guide
- [ ] Verify all packages import from new scope

## Benefits

✅ Clear official SDK identity  
✅ Prevents namespace confusion  
✅ Single source of truth for helpers  
✅ Users don't waste time on typo package  
✅ Easier maintenance (one codebase)  
✅ Unified versioning  
