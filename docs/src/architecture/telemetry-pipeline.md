# Telemetry Pipeline

The telemetry pipeline captures, transports, and stores every significant event during a run. Events flow from edges to the hive, where they are written to Parquet files for post-run analysis.

Ref: RFC-0006.

## Event Model

Every event is a `TelemetryEvent`:

```rust
struct TelemetryEvent {
    event_id: Uuid,
    edge_id: EdgeId,
    run_id: RunId,
    phase_index: u32,
    monotonic_ns: u64,
    wall_ns: u64,
    payload: EventPayload,
}
```

Two timestamps are recorded: `monotonic_ns` (for ordering within an edge) and `wall_ns` (for cross-edge correlation).

### Event Payloads

| Payload | Fields | Meaning |
|---------|--------|---------|
| `TxSubmitted` | hash, template, gas, account, nonce | Transaction sent to sequencer |
| `TxAccepted` | hash, latency_ms | Sequencer acknowledged |
| `TxIncluded` | hash, block, gas_used, latency_ms | Transaction in a block |
| `TxFailed` | hash, error, phase | Submission or confirmation failure |
| `TxTimeout` | hash, timeout_ms | Receipt polling exceeded deadline |
| `FeedConnected` | url | WebSocket connection established |
| `FeedDisconnected` | url, reason | WebSocket connection lost |
| `FeedGap` | expected_seq, actual_seq | Sequence number gap detected |
| `FeedStall` | duration_ms | No messages for threshold duration |
| `FaultInjected` | fault_type, target, handle_id | Fault activated |
| `FaultCleared` | handle_id | Fault removed |

## Edge-Side Buffer

Each edge maintains a lock-free ring buffer (default capacity: 1,000,000 events). The buffer uses `tokio::sync::mpsc` with a bounded channel.

**Backpressure:** If the buffer is full, the oldest events are dropped. A counter (`gashammer_events_dropped_total`) tracks loss. This guarantees the edge never blocks on telemetry --- transaction submission always takes priority.

## Transport

A shipper task batches events from the ring buffer and streams them to the hive via gRPC.

**Configuration** (`PipelineConfig`):

| Field | Default | Description |
|-------|---------|-------------|
| `buffer_capacity` | `1,000,000` | Ring buffer size |
| `batch_size` | `1,000` | Events per gRPC batch |
| `flush_interval_ms` | `100` | Max time before flushing a partial batch |

**Metrics:**

| Counter | Description |
|---------|-------------|
| `gashammer_events_generated` | Events created on this edge |
| `gashammer_events_shipped` | Events sent to hive |
| `gashammer_events_dropped` | Events lost to backpressure |
| `gashammer_batches_shipped` | gRPC batches sent |

## Hive-Side Storage

The hive receives event batches and writes them to Apache Parquet files.

**Partitioning:** `{data_dir}/runs/{run_id}/{hour}.parquet`

**Rotation:** by file size (default 256 MB) or time (default 1 hour), whichever comes first.

**Parquet metadata:** Each file footer includes DNA provenance fields:

| Key | Value |
|-----|-------|
| `gashammer.version` | Software version |
| `gashammer.build` | Build SHA |
| `gashammer.run_id` | Associated run UUID |
| `gashammer.copyright` | BSL-1.1 notice |

Parquet's columnar format enables efficient analytical queries over telemetry data (e.g., computing latency percentiles across millions of events).
