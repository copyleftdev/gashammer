# RFC-0006: Telemetry & Metrics Pipeline

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0003 (Hive Control Plane)
- RFC-0004 (Edge Runtime)

## Summary

This RFC defines the event model, metric taxonomy, time normalization strategy, ingestion pipeline, and storage model for all telemetry flowing through GasHammer. The telemetry pipeline must handle high-volume event streams from distributed edges while maintaining temporal accuracy across regions and avoiding becoming a bottleneck that distorts the system under test.

---

## 1. Design Constraints

1. **Telemetry must not be the first bottleneck** (PRD §11)
2. **Cross-region timestamps must be comparable** within documented confidence bounds
3. **Lossy is acceptable; silent loss is not** — dropped events must be counted and reported
4. **Raw data must be retained** for post-hoc analysis; aggregations are derived, not primary
5. **Pipeline must operate during hive↔edge disconnects** via local buffering (RFC-0004 §9)

---

## 2. Event Model

### 2.1 Canonical Event Envelope

Every event in the system conforms to this envelope:

```rust
pub struct TelemetryEvent {
    pub event_id: Uuid,
    pub edge_id: Uuid,
    pub run_id: Uuid,
    pub phase_index: u32,
    pub monotonic_ns: u64,    // quanta monotonic (edge-local)
    pub wall_ns: i64,         // chrono::Utc nanoseconds since epoch
    pub payload: EventPayload,
}
```

**Two timestamps per event:**

- `monotonic_ns` — high-resolution, monotonic, edge-local. Used for latency calculations within a single edge. Not comparable across edges.
- `wall_ns` — wall clock, NTP-synchronized. Used for cross-edge and cross-region comparisons. Subject to clock skew (see §5).

### 2.2 Event Categories

| Category | Events | Volume |
| --- | --- | --- |
| **Transaction lifecycle** | `TxSubmitted`, `TxAccepted`, `TxIncluded`, `TxFailed`, `TxTimeout` | High (1–3 per tx) |
| **Feed** | `FeedConnected`, `FeedDisconnected`, `FeedGap`, `FeedStall` | Low |
| **Fault** | `FaultInjected`, `FaultCleared` | Low |
| **Phase** | `PhaseStarted`, `BarrierReleased` | Very low |
| **Edge health** | `EdgeOverloaded` | Low |

Transaction lifecycle events dominate volume. At 10K tx/sec with 3 events/tx = 30K events/sec per edge. With 10 edges = 300K events/sec aggregate.

---

## 3. Metric Taxonomy

### 3.1 Protocol Metrics (from PRD §13)

| Metric | Unit | Source | Aggregation |
| --- | --- | --- | --- |
| `gas_submitted_per_sec` | gas/sec | Edge (TxSubmitted) | Sum across edges |
| `gas_accepted_per_sec` | gas/sec | Edge (TxAccepted) | Sum across edges |
| `gas_included_per_sec` | gas/sec | Edge (TxIncluded) | Sum across edges |
| `tx_submitted_per_sec` | tx/sec | Edge (TxSubmitted) | Sum, by workload class |
| `accept_latency_ms` | ms | Edge (TxAccepted - TxSubmitted) | p50, p95, p99, max |
| `include_latency_ms` | ms | Edge (TxIncluded - TxSubmitted) | p50, p95, p99, max |
| `batch_latency_ms` | ms | Hive (L1 monitoring) | p50, p95, p99 |
| `backlog_depth` | count | RPC polling | Point-in-time |
| `validator_lag_blocks` | blocks | L1 monitoring / node metrics | Point-in-time |
| `base_fee_gwei` | gwei | RPC (eth_gasPrice) | Time series |
| `revert_rate` | ratio | Edge (TxIncluded.status=false / total) | Per-phase |
| `feed_lag_ms` | ms | Edge (feed message time - sequencer time) | p50, p95 |

### 3.2 Infrastructure Metrics

| Metric | Unit | Source |
| --- | --- | --- |
| `target_cpu_utilization` | ratio | Prometheus scrape of target nodes |
| `target_memory_rss_bytes` | bytes | Prometheus scrape |
| `target_disk_iops` | ops/sec | Prometheus scrape |
| `target_disk_latency_ms` | ms | Prometheus scrape |
| `target_network_rtt_ms` | ms | Edge-measured (HTTP timing) |
| `target_goroutine_count` | count | Prometheus scrape (Go runtime) |

### 3.3 Distribution Metrics

| Metric | Unit | Source |
| --- | --- | --- |
| `regional_accept_latency_ms` | ms | Per-region aggregation of accept_latency |
| `regional_include_latency_ms` | ms | Per-region aggregation of include_latency |
| `regional_skew_ms` | ms | Difference between regional p50 latencies |
| `fairness_index` | 0–1 | Jain's fairness index across regions |
| `ordering_variance` | ratio | Fraction of txs received out of submission order |

### 3.4 GasHammer Self-Metrics

| Metric | Source |
| --- | --- |
| `edge_telemetry_shipped` | Edge shipper |
| `edge_telemetry_dropped` | Edge ring buffer overflow |
| `hive_telemetry_ingested` | Hive aggregator |
| `hive_telemetry_write_latency_ms` | Hive storage writer |
| `edge_conn_errors` | Edge connection pools |

---

## 4. Ingestion Pipeline

### 4.1 Architecture

```text
Edges (N)                         Hive
   │                                │
   │── gRPC StreamEvents ─────────►│
   │── gRPC StreamMetrics ────────►│
   │                                │
   │                         ┌──────┴──────┐
   │                         │ Ingestion   │
   │                         │ Router      │
   │                         └──┬───┬───┬──┘
   │                            │   │   │
   │                    ┌───────┘   │   └───────┐
   │                    ▼           ▼           ▼
   │              ┌──────────┐ ┌────────┐ ┌──────────┐
   │              │ Raw      │ │ Agg    │ │ Metrics  │
   │              │ Writer   │ │ Engine │ │ Exporter │
   │              │ (S3)     │ │        │ │ (Prom)   │
   │              └──────────┘ └────────┘ └──────────┘
```

### 4.2 Ingestion Router

Receives events from all edge gRPC streams. Routes to:

1. **Raw Writer** — appends events to per-run Parquet files in S3
2. **Aggregation Engine** — maintains in-memory sliding window aggregates
3. **Metrics Exporter** — updates Prometheus counters/histograms for live monitoring

### 4.3 Raw Storage (S3 + Parquet)

Events are buffered in memory and flushed as Parquet row groups:

- **Flush interval:** every 5 seconds or every 50K events (whichever first)
- **Partitioning:** `s3://bucket/runs/{run_id}/events/{edge_id}/{phase_index}/{chunk_id}.parquet`
- **Schema:** mirrors `TelemetryEvent` struct with columns for each `EventPayload` variant
- **Compression:** Parquet snappy compression

Parquet is chosen for:

- Columnar format — efficient for analytical queries (e.g., "all accept latencies for phase 2")
- Schema evolution — new event types add columns without breaking old readers
- Ecosystem compatibility — readable by Spark, DuckDB, Polars, pandas

### 4.4 Aggregation Engine

Maintains per-run, per-phase, per-region sliding window statistics:

```rust
pub struct RunAggregator {
    pub run_id: Uuid,
    pub windows: HashMap<AggKey, WindowState>,
}

pub struct AggKey {
    pub phase_index: u32,
    pub region: String,
    pub metric_name: String,
}

pub struct WindowState {
    pub histogram: HdrHistogram, // high-dynamic-range histogram
    pub count: u64,
    pub sum: f64,
    pub last_value: f64,
    pub window_start: Instant,
}
```

Uses `hdrhistogram` crate for accurate percentile calculation without pre-binning.

Aggregations are computed at:

- **1-second windows** — for live dashboards and Prometheus export
- **Per-phase totals** — for the run report
- **Per-run totals** — for the summary

### 4.5 Prometheus Metrics Exporter

The hive exposes `/metrics` with:

- All protocol metrics from §3.1 (aggregated across edges, labeled by region and phase)
- All self-metrics from §3.4
- Standard process metrics (memory, CPU, goroutines equivalent for Rust: tokio tasks)

Labels: `run_id`, `phase`, `region`, `edge_id`, `workload_class`

---

## 5. Time Normalization

### 5.1 Problem

Edges in different regions have different wall clocks. Even with NTP, skew of 1–10ms is common. For cross-region latency comparison, this skew can dominate the measurement.

### 5.2 Strategy

**Within a single edge:** Use `monotonic_ns` exclusively. Latency = `t_accept_mono - t_submit_mono`. This is skew-free and nanosecond-precise.

**Across edges (same region):** Use `wall_ns` with confidence bounds. Preflight skew estimation (RFC-0003 §5.2) provides a skew estimate per edge pair. Cross-edge comparisons include skew confidence in the report.

**Across regions:** Use `wall_ns` with explicit confidence degradation. Regional comparisons note:

```
us-west p99 accept: 45ms ± 3ms (skew confidence)
eu-central p99 accept: 120ms ± 5ms (skew confidence)
```

### 5.3 Skew Estimation

During preflight, the hive performs a round-trip time estimation with each edge:

1. Hive records `t1` (hive wall clock)
2. Hive sends `SkewProbe` to edge
3. Edge records `t_edge` (edge wall clock) and returns it
4. Hive records `t2` (hive wall clock)
5. Estimated skew = `t_edge - (t1 + t2) / 2`
6. Confidence = `(t2 - t1) / 2` (half the round-trip)

This is performed 10 times; median skew and max confidence are recorded.

### 5.4 Skew Annotations

Every aggregated metric that involves cross-edge comparison includes:

```rust
pub struct SkewAnnotation {
    pub estimated_skew_ns: i64,
    pub confidence_ns: u64,
    pub source_edge: Uuid,
    pub reference_edge: Uuid,
}
```

The report engine uses these to:

- Flag comparisons where confidence exceeds the measured difference
- Downgrade cross-region comparisons from "definitive" to "indicative"

---

## 6. Backpressure and Flow Control

### 6.1 Edge → Hive

- gRPC client-streaming with flow control (HTTP/2 built-in)
- If hive is slow to consume, edge's gRPC send buffer fills
- Edge detects backpressure → falls back to ring buffer
- Ring buffer overflow → drop oldest + increment counter

### 6.2 Hive Internal

- Ingestion router uses bounded tokio channels (100K event capacity)
- Raw writer flushes on interval or count threshold
- Aggregation engine is lock-free per-run (one aggregator per run, no cross-run contention)

### 6.3 Data Loss Budget

- **Target:** <0.1% event loss under normal operation
- **Under extreme load:** Up to 5% loss acceptable, but reported in run metadata
- **Zero tolerance for:** Loss of run-level aggregates (they're computed from the aggregation engine, not from raw events, so raw event loss doesn't corrupt aggregates — it reduces their precision)

---

## 7. Prometheus Scraping of Target Nodes

### 7.1 Scrape Configuration

```yaml
target_metrics:
  enabled: true
  scrape_interval: 15s
  endpoints:
    - name: sequencer
      url: "http://sequencer.staging.internal:6070/debug/metrics"
    - name: validator
      url: "http://validator.staging.internal:6070/debug/metrics"
    - name: batch-poster
      url: "http://batchposter.staging.internal:6070/debug/metrics"
```

### 7.2 Metric Selection

Not all Nitro Prometheus metrics are relevant. The hive scraper filters to a configurable allowlist. Default allowlist includes metrics matching:

- `arb_node_*` — node-level metrics
- `arb_sequencer_*` — sequencer metrics
- `arb_batchposter_*` — batch poster metrics
- `process_*` — Go process metrics (CPU, memory, goroutines)
- `go_*` — Go runtime metrics

### 7.3 Correlation

Target node metrics are stored with the same run timeline and phase annotations. The report engine can overlay:

- GasHammer-measured accept latency (from edges) vs. sequencer-reported queue depth (from Prometheus)
- GasHammer-measured gas/sec vs. target CPU utilization

---

## 8. Storage Model Summary

| Data Type | Storage | Format | Retention |
| --- | --- | --- | --- |
| Raw events | S3 | Parquet | Configurable (default 90 days) |
| Run aggregates | PostgreSQL | JSON columns | Indefinite |
| Run metadata | PostgreSQL | Relational | Indefinite |
| Prometheus snapshots | S3 | Parquet | Same as raw events |
| Artifacts | S3 | Various | Configurable (default 90 days) |

---

## 9. Risks

### Risk: Parquet write throughput becomes a bottleneck

At 300K events/sec, Parquet encoding and S3 uploads may lag.

**Mitigation:** Buffer in memory; flush asynchronously. Use multiple parallel S3 upload tasks. Parquet row groups are sized for efficient writes (target 64MB per file). If S3 uploads fail, buffer locally and retry.

### Risk: HdrHistogram memory usage under many dimensions

With many regions × phases × workload classes, histogram memory can grow.

**Mitigation:** Limit histogram precision to 3 significant digits. Cap the number of active aggregation keys. Archive completed phase histograms to Parquet.

### Risk: Prometheus metric cardinality explosion

Too many label combinations (edge_id × run_id × phase × region × class) can overwhelm Prometheus.

**Mitigation:** Prometheus metrics use only low-cardinality labels (region, phase). Per-edge and per-run detail is in Parquet, not Prometheus.

---

## 10. Open Questions

1. Should raw events be queryable in real-time (e.g., via DuckDB over S3) or only post-run?
2. Is HdrHistogram the right choice, or should we use t-digest for streaming percentiles?
3. Should the hive expose a live WebSocket feed of aggregated metrics for dashboard consumption?
4. What is the right Parquet row group size for balancing write latency vs. read efficiency?
5. Should target Prometheus scraping run from the hive or from a dedicated sidecar?

---

## 11. References

- PRD §10.4 (Measurement)
- PRD §13 (Metrics Model)
- PRD §11 (NFR — Performance: telemetry not first bottleneck)
- RFC-0001 §12 (Node Metrics — Prometheus)
- RFC-0003 §7 (gRPC — StreamEvents, StreamMetrics)
- RFC-0003 §9 (Backpressure)
- RFC-0004 §8 (Event Log)
- RFC-0004 §9 (Telemetry Shipper)
