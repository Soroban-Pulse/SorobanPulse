# Stellar & Soroban Glossary

A reference for contributors who come from general web/backend backgrounds.
Each term includes a plain-language definition, where it appears in SorobanPulse,
and a link to the upstream Stellar documentation.

---

## Core Stellar Concepts

### Ledger

A **ledger** is a single "block" in the Stellar blockchain — the immutable,
globally-agreed state of every account and contract after a round of consensus.
Ledgers are identified by a monotonically increasing integer called the **ledger
sequence number**.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `events.ledger` column (`BIGINT`) | The sequence number of the ledger that closed and contained the event |
| `GET /v1/events?from_ledger=&to_ledger=` query params | Range filters on ledger sequence |
| `START_LEDGER` env var | The ledger from which the indexer begins polling |
| `soroban_pulse_indexer_current_ledger` metric | Current indexer position |
| `soroban_pulse_indexer_latest_ledger` metric | Most recent ledger known from the RPC |

New ledgers close approximately every **5–6 seconds** on Stellar mainnet.

**Reference:** [Stellar Docs — Ledger](https://developers.stellar.org/docs/learn/glossary#ledger)

---

### TOID (Transaction Order ID)

A **TOID** is a 64-bit integer Stellar uses to uniquely identify a transaction
position within the chain. It encodes three fields packed into a single integer:

```
bits 63–32  ledger sequence number
bits 31–12  transaction index within the ledger
bits 11–0   operation index within the transaction
```

SorobanPulse extends this by appending an **event index** after a `-` separator
to produce an **event id** string (e.g. `"0000530124288000-0000000003"`). This
gives every event a globally unique, sortable identifier.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `src/toid.rs` | `parse_event_id()` decodes an RPC event id into `EventOrdinal` fields |
| `events.id` UUID | Surrogate cursor used in SSE `Last-Event-ID` (not the raw TOID) |

**Reference:** [Stellar Go source — toid package](https://github.com/stellar/go/tree/master/toid)

---

### ScVal (Stellar Contract Value)

**ScVal** is the XDR-encoded type system used by Soroban contracts. Every value
that crosses a contract boundary — function arguments, return values, event
topics, and event data — is an `ScVal`. Common variants include:

| ScVal variant | Rust/JSON representation | Typical use |
|---|---|---|
| `ScvSymbol` | `{"sym": "transfer"}` | Event topic identifying the operation |
| `ScvI128` / `ScvU128` | `{"i128": {"lo": …, "hi": …}}` | Token amounts (128-bit integer) |
| `ScvAddress` | `{"address": {"account_id": …}}` or `{"address": {"contract_id": …}}` | Participant addresses |
| `ScvBytes` | `{"bytes": "<base64>"}` | Arbitrary byte payloads |
| `ScvVec` | `{"vec": […]}` | Ordered list of ScVals |
| `ScvMap` | `{"map": [{"key":…,"val":…}, …]}` | Key-value map |

In SorobanPulse, `event_data.topic` is an array of XDR-decoded ScVals and
`event_data.value` is a single ScVal (or an object containing one).

| Where it appears in SorobanPulse | Detail |
|---|---|
| `events.event_data` JSONB column | `{"value": <ScVal>, "topic": [<ScVal>, …]}` |
| `src/scval_format.rs` | ScVal → human-readable string conversion |
| `src/xdr_validation.rs` | Validates raw XDR before insertion |

**Reference:** [Stellar Docs — ScVal XDR](https://developers.stellar.org/docs/learn/smart-contract-internals/types/built-in-types)

---

### XDR (External Data Representation)

**XDR** is the binary serialization format Stellar uses for all protocol
messages, transactions, and contract data. It is schema-defined, deterministic,
and language-agnostic.

The Soroban RPC returns event topics and data as **base64-encoded XDR strings**.
SorobanPulse decodes these into JSON before storing them in the `event_data`
JSONB column.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `src/xdr_validation.rs` | Validates that incoming XDR is well-formed |
| `src/scval_format.rs` | Converts decoded XDR ScVals to display strings |
| `docs/contract-event-schemas.md` | XDR encoding examples for each event type |

**Reference:** [Stellar Docs — XDR](https://developers.stellar.org/docs/learn/encyclopedia/network-configuration/stellar-xdr)

---

### Strkey

**Strkey** is Stellar's human-readable encoding for public keys, contract
addresses, and other binary identifiers. It uses base32 with a checksum and a
1–2 character type prefix:

| Prefix | Type | Example |
|---|---|---|
| `G` | Ed25519 public key (account) | `GAAZI4TCR3TY5OJHCTJC2A4QSY6CJWJH5IAJTGKIN2ER7LBNVKOCCWN` |
| `C` | Contract address | `CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM` |
| `S` | Ed25519 secret key | _(never stored or logged)_ |
| `M` | Muxed account | `MA7QYNF7SOWQ3GLR2BGMZEHXR3SZOG7GHPQQKOWUUXWSYUV7GUTOMS222` |

All `contract_id` values in SorobanPulse are 56-character C-prefixed Strkeys.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `events.contract_id` TEXT column | 56-character C-prefixed Strkey |
| `GET /v1/events/{contract_id}` | URL path parameter |
| `src/models/event.rs` | `ContractId` newtype validates the Strkey format |

**Reference:** [Stellar Docs — Strkey](https://developers.stellar.org/docs/learn/encyclopedia/network-configuration/stellar-transaction-submission#strkeys)

---

### SAC (Stellar Asset Contract)

A **SAC** is the built-in Soroban contract that wraps a classic Stellar asset
(e.g. XLM, USDC) and gives it a Soroban-compatible interface. SAC events follow
the **SEP-41** token interface and are the most common events SorobanPulse indexes.

The SAC address for a given asset is derived deterministically — it is a `C`-prefixed
Strkey computed from the asset code and issuer.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `src/token_events.rs` | Detects and parses `transfer`, `mint`, `burn`, `approve` events |
| `docs/sac-detection.md` | SAC detection heuristics |
| `events.contract_id` | The SAC's Strkey appears as any other `contract_id` |

**Reference:** [Stellar Docs — Stellar Asset Contract](https://developers.stellar.org/docs/tokens/stellar-asset-contract)

---

### SEP-41 (Token Interface Standard)

**SEP-41** is the Stellar Ecosystem Proposal that defines the standard interface
for fungible tokens on Soroban. Contracts that implement SEP-41 emit events with
predictable topic structures:

| Event | `topic[0]` | `topic[1]` | `topic[2]` | `event_data.value` |
|---|---|---|---|---|
| `transfer` | `sym:"transfer"` | from address | to address | amount (i128) |
| `mint` | `sym:"mint"` | admin address | to address | amount (i128) |
| `burn` | `sym:"burn"` | from address | — | amount (i128) |
| `approve` | `sym:"approve"` | from address | spender address | amount + expiry |

| Where it appears in SorobanPulse | Detail |
|---|---|
| `src/token_events.rs` | Recognises SEP-41 event shapes |
| `src/account_events.rs` | Tracks token balances derived from SEP-41 events |

**Reference:** [SEP-0041](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)

---

### TTL and Archival

On Soroban, every piece of **contract storage** (instance data, persistent
entries, temporary entries) has a **Time-To-Live (TTL)** measured in ledgers.
When TTL expires, the entry is **archived** — it is no longer accessible on-chain
without a restore operation.

**SorobanPulse does not store contract storage**, only events. However, TTL
and archival affect what the **RPC can return**:

- The Soroban RPC retains event history for a limited number of ledgers
  (the **RPC retention window**). Events older than this window cannot be
  fetched even if the ledger is still on the network.
- SorobanPulse indexes continuously so that its PostgreSQL store provides
  durable history beyond the RPC retention window.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `START_LEDGER` env var | If set beyond the RPC retention window, early history is unavailable |
| `src/indexer.rs` | Handles `getEvents` cursor exhaustion when the RPC window is exceeded |
| `docs/data-retention.md` | SorobanPulse's own data retention policies |

**Reference:** [Stellar Docs — State Archival](https://developers.stellar.org/docs/learn/smart-contract-internals/state-archival)

---

### RPC Retention Window

The Soroban RPC node only keeps event history for a finite number of ledgers
(typically a few days' worth). Once a ledger falls outside this window, its
events cannot be retrieved via `getEvents`.

SorobanPulse's value proposition is that it persists events to PostgreSQL as
they are produced, giving you queryable history indefinitely beyond the RPC
retention window.

| Where it appears in SorobanPulse | Detail |
|---|---|
| `STELLAR_RPC_URL` env var | The RPC endpoint being polled |
| `src/indexer.rs` | Polls `getEvents` and handles cursor errors |
| `src/backfill.rs` | Backfill job for gaps within the retention window |

**Reference:** [Soroban RPC — getEvents](https://developers.stellar.org/docs/data/rpc/api-reference/methods/getEvents)

---

## Soroban Events 101

### Event Structure

Every Soroban event has four fields as returned by the RPC's `getEvents` method:

```json
{
  "type":        "contract",
  "id":          "0000530124288000-0000000003",
  "contractId":  "CAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD2KM",
  "ledger":      123456,
  "ledgerClosedAt": "2026-03-14T00:00:00Z",
  "pagingToken": "0000530124288000-0000000003",
  "inSuccessfulContractCall": true,
  "topic":       ["AAAAAA==", "AAAAB8A="],
  "value":       "AAAAA..."
}
```

SorobanPulse maps this onto the `events` table as follows:

| RPC field | `events` column | Notes |
|---|---|---|
| `type` | `event_type` | `"contract"`, `"diagnostic"`, or `"system"` |
| `contractId` | `contract_id` | 56-char C-prefixed Strkey |
| `ledger` | `ledger` | Ledger sequence number |
| `ledgerClosedAt` | `timestamp` | Ledger close time (UTC) |
| `topic` (XDR array) | `event_data.topic` | XDR-decoded into JSON array of ScVals |
| `value` (XDR) | `event_data.value` | XDR-decoded into JSON ScVal |
| _(transaction hash)_ | `tx_hash` | SHA-256 hex of the enclosing transaction |
| _(generated)_ | `id` | UUID generated by SorobanPulse; used as SSE cursor |
| _(insertion time)_ | `created_at` | Wall-clock time of DB insert |

### Event Types

| `event_type` | Description |
|---|---|
| `contract` | Emitted explicitly by a contract via `env.events().publish(...)`. This is the most common type and the primary target of subscriptions. |
| `diagnostic` | Emitted by the Soroban host for debugging; only present when the RPC is in diagnostic mode. Not emitted on mainnet by default. |
| `system` | Emitted by the Soroban protocol itself (e.g. fee events). Rare in practice. |

### getEvents Pagination

The `getEvents` RPC method uses a cursor-based pagination model:

1. The indexer sends a `getEvents` request with a `startLedger` (or a
   `cursor` from a previous response).
2. The RPC returns up to 10,000 events and a `cursor` pointing to the next
   page.
3. The indexer loops until `cursor` is empty (no more events in this batch),
   then sleeps and repeats from the latest cursor.

SorobanPulse's indexer (`src/indexer.rs`) manages this cursor loop. The
`cursor_expiry_handler.rs` handles the case where a stored cursor falls
outside the RPC retention window (returns an error) — the indexer then
resets to the latest available ledger.

### RPC Retention and Gaps

The Stellar RPC keeps roughly **17,280 ledgers** of event history (≈ 24 hours
at 5 s/ledger). If SorobanPulse's indexer is offline for longer than this,
there will be a gap in the indexed history that cannot be backfilled from the
live RPC. The `src/backfill.rs` module handles partial backfills within the
retention window.

---

## Where Each Concept Appears — Quick Reference

| Term | Code | DB | API | Config |
|---|---|---|---|---|
| Ledger | `src/indexer.rs`, `src/toid.rs` | `events.ledger` | `?from_ledger=`, `?to_ledger=` | `START_LEDGER` |
| TOID | `src/toid.rs` | — (used to derive ordering) | `events[].id` in SSE | — |
| ScVal | `src/scval_format.rs`, `src/xdr_validation.rs` | `events.event_data` (JSONB) | `event_data.topic[]`, `event_data.value` | — |
| XDR | `src/xdr_validation.rs` | decoded before storage | decoded in API responses | — |
| Strkey | `src/models/event.rs` | `events.contract_id` | `/{contract_id}` path param | — |
| SAC | `src/token_events.rs` | same as any contract | same endpoints | — |
| SEP-41 | `src/token_events.rs`, `src/account_events.rs` | — | — | — |
| TTL/Archival | `src/indexer.rs` | — | — | `START_LEDGER` |
| RPC Retention | `src/indexer.rs`, `src/cursor_expiry_handler.rs` | — | — | `STELLAR_RPC_URL` |

---

## Further Reading

- [Stellar Glossary](https://developers.stellar.org/docs/learn/glossary)
- [Soroban Events docs](https://developers.stellar.org/docs/learn/smart-contract-internals/events)
- [getEvents RPC reference](https://developers.stellar.org/docs/data/rpc/api-reference/methods/getEvents)
- [SEP-0041 Token Interface](https://github.com/stellar/stellar-protocol/blob/master/ecosystem/sep-0041.md)
- [SorobanPulse schema reference](schema.md)
- [Contract event schemas](contract-event-schemas.md)
