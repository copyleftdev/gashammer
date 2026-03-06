# Nitro Integration

GasHammer interacts with Arbitrum Nitro exclusively through wire protocols. No Nitro source code is imported. This boundary is enforced at the dependency level --- `gashammer-nitro` depends only on `alloy`, `tokio-tungstenite`, and `reqwest`, never on Nitro crates.

Ref: RFC-0001.

## Integration Surface

```text
┌──────────────────────────────────────────────────────┐
│                   GasHammer Edge                      │
│                                                       │
│  ┌─────────────┐  ┌───────────────┐  ┌─────────────┐ │
│  │ RPC Provider │  │ Feed Consumer │  │ L1 Monitor  │ │
│  └──────┬──────┘  └───────┬───────┘  └──────┬──────┘ │
│         │                 │                  │        │
└─────────┼─────────────────┼──────────────────┼────────┘
          │                 │                  │
    JSON-RPC/HTTP     WebSocket            JSON-RPC/HTTP
    (port 8547)       (port 9642)          (L1 RPC)
          │                 │                  │
    ┌─────┴──────┐   ┌─────┴──────┐    ┌──────┴───────┐
    │ Sequencer  │   │ Feed Relay │    │  L1 (Geth)   │
    │  / Gateway │   │            │    │ SequencerInbox│
    └────────────┘   └────────────┘    │ RollupCore   │
                                       └──────────────┘
```

## JSON-RPC Provider

`NitroRpcProvider` wraps an alloy HTTP transport with retry logic and health checks.

**Configuration** (`RpcConfig`):

| Field | Default | Description |
|-------|---------|-------------|
| `url` | required | Sequencer or gateway endpoint |
| `timeout_ms` | `5000` | Per-request timeout |
| `max_retries` | `3` | Retry count on transient errors |
| `retry_base_delay_ms` | `500` | Base delay for exponential backoff |

**Key operations:**

- `send_raw_transaction(raw_tx)` --- submit a signed transaction
- `get_transaction_receipt(hash)` --- poll for inclusion
- `get_block_number()` --- current L2 block height
- `health_check()` --- validates connectivity via `eth_chainId`

Every outbound HTTP request includes the `User-Agent` and `X-Powered-By` headers with GasHammer version metadata (see [DNA Provenance](../reference/dna-provenance.md)).

## Sequencer Feed Consumer

`FeedConsumer` connects to the Nitro feed relay over WebSocket (port 9642) and parses the `BroadcastMessage` envelope.

**Wire format** (JSON over WebSocket):

```json
{
  "version": 1,
  "messages": [
    {
      "sequenceNumber": 12345,
      "message": { "header": {...}, "l2Msg": "0x..." },
      "signature": "0x..."
    }
  ],
  "confirmedSequenceNumberMessage": { "sequenceNumber": 12340 }
}
```

**Behavior:**

- Monitors sequence number gaps and emits `FeedGap` telemetry events.
- Detects stalls when no message arrives within `stall_threshold_ms` (default: 30,000).
- On disconnect, reconnects with exponential backoff (base 1s, max 30s, jitter).
- Tracks `gashammer_feed_messages_total` and `gashammer_feed_reconnects_total` counters.

**Configuration** (`FeedConfig`):

| Field | Default | Description |
|-------|---------|-------------|
| `url` | required | Feed relay WebSocket URL |
| `channel_buffer` | `4096` | Internal channel capacity |
| `ping_interval_ms` | `30000` | WebSocket ping interval |
| `pong_timeout_ms` | `5000` | Pong deadline |
| `stall_threshold_ms` | `30000` | Silence before stall detection |

## L1 Contract Reader

`L1ContractReader` reads Nitro's L1 contracts via an Ethereum JSON-RPC provider.

**Contracts:**

| Contract | Key Functions / Events |
|----------|----------------------|
| `SequencerInbox` | `batchCount()`, `event SequencerBatchDelivered(...)` |
| `RollupCore` | `latestConfirmed()`, `event AssertionCreated(...)`, `event AssertionConfirmed(...)` |

**Decoded event types:**

- `BatchDeliveredEvent` --- batch sequence number, accumulators, time bounds, data location, L1 block/tx
- `AssertionEvent` --- assertion hash, kind (Created/Confirmed), L1 block/tx

**Configuration** (`L1Config`):

| Field | Default | Description |
|-------|---------|-------------|
| `rpc_url` | required | L1 Ethereum RPC endpoint |
| `sequencer_inbox_address` | required | Hex address of SequencerInbox |
| `rollup_core_address` | required | Hex address of RollupCore |
| `poll_interval_ms` | `15000` | Polling interval |
| `lookback_blocks` | `100` | Blocks to scan on first poll |

## L1 Monitor

`L1Monitor` spawns a background `tokio` task that polls the L1 contracts at the configured interval and emits events via an `mpsc` channel.

**Event types:**

| Variant | Payload | Meaning |
|---------|---------|---------|
| `BatchDelivered` | `Vec<BatchDeliveredEvent>`, block | New batches posted to L1 |
| `AssertionUpdate` | `Vec<AssertionEvent>`, block | Assertion created or confirmed |
| `PollError` | error message | Non-fatal poll failure (retries next tick) |

**Metrics** (atomic counters):

| Counter | Description |
|---------|-------------|
| `gashammer_l1_polls` | Total poll iterations |
| `gashammer_l1_batches_seen` | Batch events observed |
| `gashammer_l1_assertions_seen` | Assertion events observed |
| `gashammer_l1_poll_errors` | Poll errors encountered |

The monitor exits when the event channel is closed, enabling clean shutdown via dropping the receiver.
