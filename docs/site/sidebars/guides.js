// @ts-check
// docs/site/sidebars/guides.js
// Sidebar for the "Guides" section (plugin-content-docs id: 'guides').
// Mirrors the IA defined in docs/site/SUMMARY.md §2.

/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  guides: [
    // ── 2.1 API Usage ──────────────────────────────────────────────────────
    {
      type: 'category',
      label: 'API Usage',
      collapsed: false,
      items: [
        'api-guide',
        'api-usage',
        'api-versioning',
        'api-deprecation',
        'api-changelog',
        'rate_limiting',
        'rate-limiting-advanced',
        'cursor-pagination',
        'idempotency',
        'http-caching',
        'compression-optimization',
        'postman',
        'api-sandbox',
        'api-cookbook',
      ],
    },

    // ── 2.2 Event Indexing ──────────────────────────────────────────────────
    {
      type: 'category',
      label: 'Event Indexing',
      items: [
        'architecture',
        'contract-event-schemas',
        'event-normalization',
        'event-deduplication',
        'event_deduplication_replicas',
        'event-aggregation',
        'event-tagging',
        'event-encryption',
        'batch-operations',
        'bulk_export',
        'advisory-lock-behavior',
        'filter-dsl',
        'temporal-queries',
        'contract-event-simulation',
        'event-simulator',
        'cross-chain-events',
        'cross_chain_correlation',
      ],
    },

    // ── 2.3 Streaming ───────────────────────────────────────────────────────
    {
      type: 'category',
      label: 'Streaming (SSE)',
      items: [
        'sse-reconnection',
        'sse_reverse_proxy_configuration',
        'streaming-optimization',
        'STREAMING_RESPONSES',
        'query-streaming',
        'stream-statistics',
        'event-feeds',
      ],
    },

    // ── 2.4 Webhooks ────────────────────────────────────────────────────────
    {
      type: 'category',
      label: 'Webhooks',
      items: [
        'webhook-verification',
        'webhook-signing',
        'webhook_circuit_breaker',
        'webhook-endpoint-rate-limits',
        'webhook-logging',
        'subscription-best-practices',
        'subscription-validator',
        'retry-policies',
        'priority-queueing',
      ],
    },

    // ── 2.5 Notifications ───────────────────────────────────────────────────
    {
      type: 'category',
      label: 'Notifications',
      items: [
        'notification-architecture',
        'notification-channels',
        'notification-features',
        'notification-rate-limiting',
        'notification-batching',
        'notification-deduplication',
        'notification-content-filter',
        'notification-delivery-receipts',
        'delivery-receipts',
        'email-notifications',
        'email-quick-start',
        'slack-integration',
        'discord-integration',
        'teams-integration',
        'pagerduty-integration',
      ],
    },

    // ── 2.6 SDKs & Client Libraries ─────────────────────────────────────────
    {
      type: 'category',
      label: 'SDKs & Clients',
      items: [
        'sdk-integration-guide',
        'sdk-development',
        'client-libraries',
        'js-sdk',
        'python-sdk',
        'openapi-codegen',
        'codegen',
        'cli-usage',
        'vscode-extension',
      ],
    },

    // ── 2.7 Operations ──────────────────────────────────────────────────────
    {
      type: 'category',
      label: 'Operations',
      items: [
        'deployment',
        'deployment-platforms',
        'kubernetes-probes',
        'terraform',
        'multi-region',
        'multi-deployment-architecture',
        'gitops',
        'metrics-reference',
        'alerting',
        'tracing',
        'logging',
        'sli-slo',
        'troubleshooting-guide',
        'troubleshooting',
        {
          type: 'category',
          label: 'Runbooks',
          items: [
            'runbooks/operator-runbook',
            'runbooks/indexer-lag',
            'runbooks/db-pool-exhaustion',
            'runbooks/rpc-errors',
            'runbooks/webhook-failures',
            'runbooks/notifications',
            'runbooks/sse-connections',
          ],
        },
        {
          type: 'category',
          label: 'Emergency Runbooks',
          items: [
            'emergency-runbooks/security-breach-response',
            'emergency-runbooks/database-corruption-recovery',
            'emergency-runbooks/data-loss-recovery',
            'emergency-runbooks/service-failure-recovery',
          ],
        },
      ],
    },

    // ── 2.8 Migration Guides ────────────────────────────────────────────────
    {
      type: 'category',
      label: 'Migration Guides',
      items: [
        'migration-guides/README',
        'migration-guides/from-stellar-horizon',
        'migration-guides/from-other-indexers',
        'migration-guides/data-migration-procedures',
        'migration-guides/subscription-mapping',
        'migration-guides/rollback-procedures',
        'migration-guides/validation',
      ],
    },

    // ── 2.9 Contributing & Design ───────────────────────────────────────────
    {
      type: 'category',
      label: 'Contributing',
      items: [
        'development-setup',
        {
          type: 'category',
          label: 'Architecture Decisions',
          items: [
            'adr/README',
            'adr/0001-adr-system',
            'adr/0002-multi-replica-indexing',
            'adr/0003-webhook-retry-strategy',
            'adr/0004-event-compression',
            'adr/review-guidelines',
          ],
        },
        'design-tokens',
        'diagram-style-guide',
      ],
    },
  ],
};

module.exports = sidebars;
