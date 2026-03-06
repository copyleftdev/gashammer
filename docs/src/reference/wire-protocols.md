# Wire Protocols

GasHammer communicates with Nitro and between its own components through four wire protocols. This page documents each protocol's usage, message format, and configuration.

## JSON-RPC (Edge → Nitro)

Standard Ethereum JSON-RPC over HTTP. Used for transaction submission and state queries.

**Endpoint:** Sequencer (port 8547) or Gateway (port 8547).

### Methods Used

| Method | Direction | Purpose |
|--------|-----------|---------|
| `eth_sendRawTransaction` | Edge → Sequencer | Submit signed transactions |
| `eth_getTransactionReceipt` | Edge → Gateway | Poll for confirmation |
| `eth_blockNumber` | Edge → Gateway | Current L2 block height |
| `eth_chainId` | Edge → Sequencer | Health check and chain validation |
| `eth_gasPrice` | Edge → Gateway | Current gas price |
| `eth_getTransactionCount` | Edge → Gateway | Nonce recovery on failure |
| `eth_call` | Edge → Gateway | Read-only contract calls |
| `eth_getLogs` | Monitor → L1 | L1 event queries |

### Request Format

```json
{
  "jsonrpc": "2.0",
  "method": "eth_sendRawTransaction",
  "params": ["0x<signed_tx_rlp>"],
  "id": 1
}
```

### Headers

Every request includes:

| Header | Value |
|--------|-------|
| `User-Agent` | `GasHammer/<version> (<build_sha>)` |
| `X-Powered-By` | `GasHammer/<version>` |
| `X-GasHammer-Version` | `<version>` |
| `X-GasHammer-Build` | `<build_sha>` |

## WebSocket Feed (Edge → Feed Relay)

The sequencer feed is a one-way WebSocket stream from the feed relay (port 9642) to the edge. Messages are JSON-encoded `BroadcastMessage` envelopes.

### Message Format

```json
{
  "version": 1,
  "messages": [
    {
      "sequenceNumber": 12345,
      "message": {
        "header": {
          "kind": 3,
          "poster": "0x...",
          "blockNumber": 100,
          "timestamp": 1700000000,
          "requestId": "0x...",
          "l1BaseFeeEstimate": "0x..."
        },
        "l2Msg": "0x..."
      },
      "signature": "0x..."
    }
  ],
  "confirmedSequenceNumberMessage": {
    "sequenceNumber": 12340
  }
}
```

### Reconnection

On disconnect, the feed consumer reconnects with exponential backoff:

| Attempt | Delay |
|---------|-------|
| 1 | 1s + jitter |
| 2 | 2s + jitter |
| 3 | 4s + jitter |
| n | min(2^n, 30)s + jitter |

## gRPC (Edge ↔ Hive)

Internal communication between edges and the hive uses gRPC with Protocol Buffers. Definitions are in `proto/gashammer/v1/`.

### Service: HiveEdge

| RPC | Direction | Description |
|-----|-----------|-------------|
| `RegisterEdge` | Edge → Hive | Register with capabilities |
| `Heartbeat` | Edge → Hive | Periodic health signal |
| `StreamTelemetry` | Edge → Hive | Bidirectional event stream |
| `ControlStream` | Hive → Edge | Run start/stop commands |

### Proto Files

| File | Contents |
|------|----------|
| `hive_edge.proto` | `HiveEdge` service, `RegisterEdgeRequest/Response`, `ControlMessage` |
| `telemetry.proto` | `TelemetryEvent`, `TelemetryBatch`, event payload messages |
| `types.proto` | Shared types: `RunId`, `EdgeId`, `PhaseConfig`, `EdgeStatus` |

### Transport

- Default port: 9090
- TLS: mTLS recommended for production (configured via `tls.cert_path`, `tls.key_path`, `tls.ca_path`)
- Keepalive: 30s interval

## L1 JSON-RPC (Monitor → L1 Geth)

The L1 monitor polls an Ethereum L1 node for Nitro contract events.

### Methods Used

| Method | Purpose |
|--------|---------|
| `eth_blockNumber` | Current L1 block for range queries |
| `eth_getLogs` | Fetch `SequencerBatchDelivered` and assertion events |
| `eth_call` | Read `batchCount()` and `latestConfirmed()` |

### Event Signatures

| Event | Signature Hash |
|-------|---------------|
| `SequencerBatchDelivered(uint256,bytes32,bytes32,bytes32,uint256,(uint64,uint64),uint8)` | Computed via `keccak256` |
| `AssertionCreated(bytes32,...)` | Computed via `keccak256` |
| `AssertionConfirmed(bytes32,...)` | Computed via `keccak256` |

All signature hashes are verified non-zero and mutually distinct in unit tests.
