# Cross-Chain Event Correlation

`src/cross_chain_correlation.rs` correlates events across multiple
blockchain networks — detecting causal relationships, grouping related
events into a trace, and now (Issue #935) persisting and querying that
data via `migrations/20260831000003_cross_chain_event_correlation.sql`.

## Network identifier on events

`events.network` (and the corresponding `Event::network` field in
`src/models.rs`) records which chain/network an event was indexed from
(e.g. `soroban-mainnet`, `soroban-testnet`), defaulting to
`soroban-mainnet` for backward compatibility. This is the join key used
throughout the rest of this module.

## In-memory correlation detection

`CorrelationEngine::calculate_similarity` scores two `TraceEvent`s by
event type (0.4), contract id (0.3), and ledger proximity (0.3).
`CorrelationEngine::detect_causality` classifies a pair as:

- `Sequential` — same chain, source occurs before target,
- `Direct` — different chains, similarity ≥ `similarity_threshold`
  (default `0.75`),
- `None` — otherwise.

`CrossChainTraceBuilder` assembles a `CrossChainTrace`: a root
transaction, its events (ordered by `depth`), the correlations detected
between them, and the sequence of chains the flow touched.

## Persistence and querying

`persist_correlation(pool, ...)` upserts one correlation edge into
`cross_chain_correlations`, keyed on `(source_event_id, target_event_id)`
— re-detecting the same pair updates confidence/causality/reason rather
than duplicating the row.

`query_correlations(pool, &CorrelationQuery { network, min_confidence,
causality, limit, offset })` filters persisted correlations; `network`
matches either side of the edge, since a correlation "involving" a
network should surface regardless of whether it was the source or
target.

## Correlation metrics

`correlation_metrics(pool)` returns `CorrelationMetrics`: total
correlation count, average confidence, a breakdown by causality type, and
a breakdown by `"{source_network}->{target_network}"` pair — useful for
spotting which network pairs correlate most and whether confidence is
trending down (noisier matching) over time.

## Network-specific deduplication

`is_network_duplicate(pool, network, fingerprint)` checks the existing
`fingerprint` deduplication (see `src/dedup.rs`, Issue #582) scoped to a
single `network`. The same content fingerprint appearing independently on
two different networks is a legitimate coincidence, not a duplicate — so
dedup lookups must always be scoped by network, never global.

## Cross-chain event grouping

`create_event_group(pool, label, &event_ids)` creates a
`cross_chain_event_groups` row and attaches the given events as members
via `cross_chain_event_group_members`, for flows that should be grouped
under one banner independent of the pairwise correlation graph (e.g. "all
legs of this bridge transfer"). `event_group_members(pool, group_id)`
returns a group's event ids.

## Testing

`src/cross_chain_correlation.rs` includes unit tests for
`TransactionId`/`EventCorrelation` construction, similarity scoring,
`CrossChainTraceBuilder`, causality classification (including that
same-chain pairs are always `Sequential`/`None`, never the cross-chain-only
`Direct`, and that dissimilar cross-chain pairs yield no causality), and
`CorrelationQuery`'s defaults.
