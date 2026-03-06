# Data Flow

This page describes how data moves through GasHammer during a run.

## Event Lifecycle

Every significant action produces a telemetry event. Events flow from edges to the hive, where they are stored and analyzed.

```text
Edge                           Network              Hive
────                           ───────              ────

TxSubmitted ──────────────────────────────────────▶ Ring Buffer
  (hash, template, gas,                              │
   account, timestamp)                                │
                                                      ▼
TxConfirmed ──────────────────────────────────────▶ Parquet Writer
  (hash, block, gas_used,                            │
   latency_ms)                                       │
                                                     ▼
FeedCorrelation ──────────────────────────────────▶ Oracle Evaluator
  (hash, inclusion_latency_ms,                       │
   sequence_number)                                  │
                                                     ▼
OracleVerdict ────────────────────────────────────▶ Report Aggregator
  (invariant, pass/fail,                             │
   evidence)                                         │
                                                     ▼
                                               RunReport (JSON/Markdown)
```

## Telemetry Transport

Events are transported from edges to the hive via **gRPC streaming**:

1. Edge batches events from its ring buffer (configurable batch size, default 1024)
2. Edge sends the batch to the hive via a bidirectional gRPC stream
3. Hive acknowledges receipt (enables backpressure)
4. Hive writes events to Parquet files partitioned by `run_id` and hourly time buckets

## Parquet Storage

Telemetry data is stored in Apache Parquet format for efficient columnar access:

- **Partitioning:** `{data_dir}/runs/{run_id}/{hour}.parquet`
- **Rotation:** by size (default 256 MB) or time (default 1 hour)
- **Metadata:** each Parquet file footer includes GasHammer DNA fields:
  - `gashammer.version` --- software version
  - `gashammer.build` --- build SHA
  - `gashammer.run_id` --- associated run
  - `gashammer.copyright` --- BSL-1.1 notice

## Report Generation

After a run completes, the report engine reads events from Parquet and computes:

| Metric | Source Events | Output |
|--------|--------------|--------|
| Latency percentiles | TxConfirmed | p50, p90, p95, p99, p99.9 |
| Throughput time series | TxSubmitted, TxConfirmed | gas/sec and tx/sec over time |
| Inclusion latency | FeedCorrelation | Feed inclusion timing |
| Revert rate | TxConfirmed (status=reverted) | % of failed transactions |
| Correctness verdicts | OracleVerdict | Pass/fail per invariant |
| Capacity envelope | All latency events | Safe operating envelope boundaries |

## Metrics (Prometheus)

Both hive and edge expose Prometheus metrics on their configured ports. All metrics use the `gashammer_` prefix.

### Edge Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `gashammer_tx_submitted_total` | counter | Transactions submitted |
| `gashammer_tx_confirmed_total` | counter | Transactions confirmed |
| `gashammer_tx_failed_total` | counter | Transactions failed |
| `gashammer_tx_latency_seconds` | histogram | Transaction confirmation latency |
| `gashammer_feed_messages_total` | counter | Feed messages received |
| `gashammer_correlation_latency_seconds` | histogram | Feed inclusion latency |
| `gashammer_events_generated_total` | counter | Telemetry events created |
| `gashammer_events_dropped_total` | counter | Events dropped due to buffer full |

### Hive Metrics

| Metric | Type | Description |
|--------|------|-------------|
| `gashammer_edges_active` | gauge | Currently connected edges |
| `gashammer_runs_active` | gauge | Currently running tests |
| `gashammer_events_received_total` | counter | Events received from edges |
| `gashammer_oracle_checks_total` | counter | Invariant checks performed |
| `gashammer_oracle_violations_total` | counter | Invariant violations detected |
