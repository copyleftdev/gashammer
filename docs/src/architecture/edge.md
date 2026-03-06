# Edge Runtime

Edges are GasHammer's regional worker processes. Each edge connects to a Nitro deployment and generates transaction load as directed by the hive.

## Startup Sequence

1. Parse CLI arguments and load `edge.toml` configuration
2. Connect to the Nitro sequencer (JSON-RPC) and feed relay (WebSocket)
3. Register with the hive via gRPC
4. Start the heartbeat task
5. Wait for run assignments

## Transaction Pipeline

The tx pipeline is the core of the edge runtime:

```text
Workload Engine ──▶ Build Tx ──▶ Sign ──▶ Submit ──▶ Track Receipt
       │                                      │              │
       │              Account Pool ────────────┘              │
       │                                                      │
       └──────────── Feed Correlator ◀────────────────────────┘
                          │
                    Telemetry Events
```

1. **Workload engine** selects a template and gas rate based on the current phase
2. **Build** constructs a transaction from the template and an account from the pool
3. **Sign** applies the account's private key
4. **Submit** sends the signed tx to the sequencer via JSON-RPC
5. **Track** polls for the receipt and records confirmation latency
6. **Feed correlator** matches submitted txs against their appearance in the sequencer feed, measuring inclusion latency

## Account Pool

Test accounts are derived deterministically from an HD wallet mnemonic at the derivation path `m/44'/60'/0'/0'/0x4748/{index}` (the `0x4748` segment is the GasHammer DNA marker --- "GH" in hex).

Accounts are partitioned across edges with no overlap. Each account tracks its nonce locally to avoid on-chain nonce queries. On transaction failure, the nonce is recovered.

## Feed Correlator

The correlator maintains a set of pending transaction hashes with their submission timestamps. As messages arrive on the sequencer feed, it matches them against the pending set and emits correlation events with the measured inclusion latency.

Transactions that never appear in the feed within the timeout are recorded as correlation misses.

## Telemetry

The edge maintains a lock-free ring buffer of telemetry events. A shipper task batches events and streams them to the hive via gRPC. If the buffer is full, the oldest events are dropped (with a counter to track loss).

## Graceful Shutdown

On SIGTERM or SIGINT:
1. Stop the workload engine (no new transactions)
2. Wait for pending transactions to complete or timeout
3. Flush remaining telemetry to the hive
4. Deregister from the hive
5. Exit

## Configuration Reference

See [Edge Configuration](../getting-started/configuration.md#edge-configuration).
