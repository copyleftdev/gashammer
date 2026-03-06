# RFC-0004: Edge Runtime

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0002 (Technology Stack & Dependencies)
- RFC-0003 (Hive Control Plane)

## Summary

This RFC defines the architecture of the **Edge** — GasHammer's regionally deployed worker process. Each edge executes its assigned workload slice, maintains connections to target Nitro infrastructure, collects local measurements, ships telemetry to the hive, and optionally runs fault injection adapters. The edge is designed for high-throughput, low-overhead operation with minimal measurement distortion.

---

## 1. Responsibilities

1. **Hive coordination** — register, heartbeat, receive workload slices, participate in barriers, report status
2. **Transaction generation** — construct, sign, and submit transactions per assigned workload slice
3. **Feed subscription** — maintain WebSocket connection to sequencer feed relay; correlate accepted transactions
4. **Receipt tracking** — poll for transaction receipts to measure inclusion latency
5. **Local measurement** — record timestamps for every lifecycle event with monotonic precision
6. **Telemetry shipping** — stream events and metrics to hive via gRPC
7. **Fault injection** — execute locally-scoped fault primitives when instructed by the scenario
8. **Local buffering** — buffer telemetry during hive disconnects; replay on reconnection

---

## 2. Process Architecture

```text
┌──────────────────────────────────────────────────────────────┐
│                      gashammer-edge                          │
│                                                              │
│  ┌──────────────┐                    ┌───────────────────┐   │
│  │ Hive Client  │◄──── gRPC ────────►│  Hive (remote)    │   │
│  │ (tonic)      │                    └───────────────────┘   │
│  └──────┬───────┘                                            │
│         │ control messages                                   │
│  ┌──────┴────────────────────────────────────────────────┐   │
│  │                   Run Executor                        │   │
│  │                                                       │   │
│  │  ┌──────────────┐  ┌──────────────┐  ┌────────────┐  │   │
│  │  │ Tx Generator │  │ Feed         │  │ Receipt    │  │   │
│  │  │  sign+submit │  │ Consumer     │  │ Tracker    │  │   │
│  │  └──────┬───────┘  └──────┬───────┘  └─────┬──────┘  │   │
│  │         │                 │                 │         │   │
│  │  ┌──────┴─────────────────┴─────────────────┴──────┐  │   │
│  │  │              Event Log (ring buffer)             │  │   │
│  │  └──────────────────────┬──────────────────────────┘  │   │
│  └─────────────────────────┼─────────────────────────────┘   │
│                            │                                 │
│  ┌─────────────────────────┴──────────────────────────────┐  │
│  │              Telemetry Shipper                         │  │
│  │  batch + compress → gRPC stream to hive                │  │
│  └────────────────────────────────────────────────────────┘  │
│                                                              │
│  ┌────────────────────────────────────────────────────────┐  │
│  │              Fault Adapter (optional)                  │  │
│  │  netem, tc, cgroup, process signals                    │  │
│  └────────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────────┘
```

Single tokio-based async process. All subsystems are concurrent task groups sharing an event bus.

---

## 3. Startup and Registration

### 3.1 Bootstrap Sequence

1. Parse config (file + env + CLI)
2. Initialize monotonic clock (`quanta`)
3. Establish mTLS connection to hive gRPC endpoint
4. Send `RegisterEdge` with bootstrap token + self-description
5. Receive `edge_id` assignment
6. Start heartbeat stream
7. Enter idle loop — await workload assignment

### 3.2 Self-Description

```rust
pub struct EdgeSelfDescription {
    pub region: String,
    pub capacity_class: CapacityClass,
    pub software_version: semver::Version,
    pub supported_adapters: Vec<String>,
    pub instance_profile: InstanceProfile,
}

pub struct InstanceProfile {
    pub cpu_cores: u32,
    pub memory_mb: u64,
    pub network_class: NetworkClass, // 1g, 10g, 25g, unknown
    pub arch: String,                // x86_64, aarch64
}
```

---

## 4. Connection Management

### 4.1 Connection Pool Architecture

| Pool | Target | Protocol | Purpose |
| --- | --- | --- | --- |
| **Sequencer Pool** | Sequencer RPC endpoint | HTTP/1.1 keep-alive | `eth_sendRawTransaction` submission |
| **Gateway Pool** | RPC gateway endpoint | HTTP/1.1 keep-alive | Receipt polling, gas price, state reads |
| **Feed Connection** | Feed relay | WebSocket (port 9642) | Sequencer feed subscription |

### 4.2 Sequencer Pool

- **Implementation:** `reqwest::Client` with connection pooling
- **Pool size:** Configurable; default = 2× CPU cores
- **Keep-alive:** Enabled; idle timeout 30s
- **Timeout:** Configurable per-request; default 5s for submission
- **Retry policy:** No automatic retry for `eth_sendRawTransaction` — retries risk duplicate submissions with stale nonces. Log failure and advance nonce tracking.

### 4.3 Gateway Pool

- **Implementation:** `alloy::provider::ProviderBuilder` with HTTP transport
- **Pool size:** Configurable; default = CPU cores
- **Retry policy:** Idempotent reads retried (3 attempts, exponential backoff from 100ms)

### 4.4 Feed Connection

- **Implementation:** `tokio_tungstenite::connect_async`
- **Single connection** per edge to the feed relay
- **Reconnection:** Automatic with exponential backoff (100ms → 1s → 5s → 10s cap)
- **Ping/pong:** WebSocket ping every 10s; disconnect if no pong within 5s

### 4.5 Connection Health Metrics

- `edge_conn_active{pool="sequencer|gateway"}` — current active connections
- `edge_conn_errors{pool="sequencer|gateway"}` — connection error count
- `edge_feed_connected` — boolean gauge
- `edge_feed_reconnects` — reconnection count

---

## 5. Transaction Generation Pipeline

### 5.1 Pipeline Stages

```text
WorkloadSlice
    │
    ▼
┌──────────────┐
│ Rate Limiter │  gas/sec or tx/sec token bucket
└──────┬───────┘
       ▼
┌──────────────┐
│ Template     │  select by weighted distribution (seeded PRNG)
│ Selector     │
└──────┬───────┘
       ▼
┌──────────────┐
│ Account      │  round-robin from disjoint pool (seeded PRNG)
│ Selector     │
└──────┬───────┘
       ▼
┌──────────────┐
│ Tx Builder   │  populate to/data/value/gas/nonce/chain_id (alloy)
└──────┬───────┘
       ▼
┌──────────────┐
│ Signer       │  secp256k1, EIP-1559 type (alloy-signer-local)
└──────┬───────┘
       ▼
┌──────────────┐
│ Submitter    │  POST eth_sendRawTransaction; record t_send
└──────┬───────┘
       ▼
┌──────────────┐
│ Event Log    │  TxSubmitted event with all metadata
└──────────────┘
```

### 5.2 Rate Limiting

Token bucket algorithm:

```rust
pub struct RateLimiter {
    mode: RateMode,
    tokens: AtomicU64,
    refill_rate: u64,       // tokens per second
    burst_capacity: u64,
    last_refill: AtomicU64, // monotonic timestamp
}

pub enum RateMode {
    GasPerSecond(u64),  // each tx consumes its gas_limit in tokens
    TxPerSecond(u64),   // each tx consumes 1 token
    Unlimited,          // stress mode
}
```

During ramp phase, `refill_rate` increases linearly. During sustain, it holds steady. During recover, it drops to zero.

### 5.3 Account Pool

Each edge owns a disjoint set of pre-funded accounts (RFC-0003 §4.3):

```rust
pub struct AccountPool {
    accounts: Vec<Account>,
    next_index: AtomicUsize,
}

pub struct Account {
    pub address: Address,
    pub private_key: SigningKey,
    pub nonce: AtomicU64,   // initialized from eth_getTransactionCount at preflight
}
```

**Nonce management:**

- Initialize from `eth_getTransactionCount(address, "pending")` during preflight
- Increment locally after each submission (atomic)
- On nonce error from RPC: re-sync from chain, log discrepancy
- **No two edges share an account** — enforced by scenario compiler

### 5.4 Deterministic Generation

For reproducibility (PRD §10.3, §11):

- Edge PRNG seeded with `master_seed XOR edge_index`
- Template selection: `ChaCha20Rng::seed_from_u64(edge_seed)`
- Account selection: same PRNG sequence
- Same seed + same edge count + same scenario → identical tx sequence per edge

---

## 6. Feed Consumer

### 6.1 Purpose

Subscribes to sequencer feed (RFC-0001 §4) to:

1. Detect when submitted txs achieve soft finality
2. Record `t_accept` for acceptance latency
3. Track chain head progression
4. Detect feed anomalies (gaps, stalls, disconnections)

### 6.2 Correlation Engine

```rust
pub struct FeedCorrelator {
    pending: DashMap<TxHash, TxSubmitRecord>,
    eviction_timeout: Duration, // max age before dropping unmatched
}

pub struct TxSubmitRecord {
    pub tx_hash: TxHash,
    pub submit_monotonic_ns: u64,
    pub gas_limit: u64,
    pub workload_class: String,
    pub account: Address,
}
```

**Flow:**

1. Tx Generator submits tx → inserts `TxSubmitRecord` into `pending` keyed by `tx_hash`
2. Feed Consumer receives `BroadcastFeedMessage` → decodes `l2Msg` via `alloy::rlp` → extracts tx hashes
3. Lookup in `pending`; if found: emit `TxAccepted` event, remove from pending
4. If not found: ignore (tx from another source or system tx)

### 6.3 Feed Metrics

| Metric | Type | Description |
| --- | --- | --- |
| `edge_feed_messages_received` | Counter | Total feed messages |
| `edge_feed_tx_correlated` | Counter | Matched to pending submissions |
| `edge_feed_sequence_gaps` | Counter | Sequence number discontinuities |
| `edge_feed_stall_seconds` | Histogram | Stall durations (>1s with no message) |
| `edge_feed_lag_ms` | Gauge | Delay between sequencer ordering and edge receipt |

---

## 7. Receipt Tracker

### 7.1 Polling Strategy

```rust
pub struct ReceiptTracker {
    pending: DashMap<TxHash, ReceiptPending>,
    provider: Arc<dyn Provider>,
    config: ReceiptConfig,
}

pub struct ReceiptConfig {
    pub initial_delay: Duration,  // 500ms
    pub poll_interval: Duration,  // 250ms (matches block time)
    pub max_wait: Duration,       // 60s
    pub batch_size: usize,        // batch N receipts per RPC call
}
```

**Flow:**

1. After `TxSubmitted`, enqueue in receipt tracker
2. After `initial_delay`, begin polling `eth_getTransactionReceipt`
3. Poll every `poll_interval` until non-null or `max_wait` exceeded
4. On receipt: emit `TxIncluded` with block number, gas used, status
5. On timeout: emit `TxTimeout`

### 7.2 Receipt Data

```rust
pub struct TxReceiptRecord {
    pub tx_hash: TxHash,
    pub block_number: u64,
    pub gas_used: u64,
    pub effective_gas_price: u128,
    pub status: bool,
    pub logs_count: usize,
    pub include_monotonic_ns: u64,
}
```

### 7.3 Batched Polling

Group pending hashes into batches; use JSON-RPC batch requests where supported. Stagger to spread load.

---

## 8. Event Log

### 8.1 Design

Central data structure. All subsystems write; telemetry shipper reads.

```rust
pub struct EventLog {
    buffer: Arc<ArrayQueue<TelemetryEvent>>, // crossbeam lock-free MPSC ring
    capacity: usize,
    dropped: AtomicU64,
}
```

If buffer fills (shipper slow), oldest events drop and `dropped` counter increments.

### 8.2 Event Types

```rust
pub enum EventPayload {
    TxSubmitted {
        tx_hash: TxHash,
        account: Address,
        gas_limit: u64,
        workload_class: String,
        nonce: u64,
    },
    TxAccepted {
        tx_hash: TxHash,
        sequence_number: u64,
        feed_lag_ns: u64,
    },
    TxIncluded {
        tx_hash: TxHash,
        block_number: u64,
        gas_used: u64,
        effective_gas_price: u128,
        success: bool,
    },
    TxFailed { tx_hash: TxHash, error: String, stage: FailureStage },
    TxTimeout { tx_hash: TxHash, stage: FailureStage, waited_ms: u64 },
    FeedConnected,
    FeedDisconnected { reason: String },
    FeedGap { from: u64, to: u64 },
    FeedStall { duration_ms: u64 },
    FaultInjected { fault_type: String, target: String },
    FaultCleared { fault_type: String, target: String },
    PhaseStarted { phase_index: u32, phase_name: String },
    BarrierReleased { run_id: String, start_ns: u64 },
    EdgeOverloaded { metric: String, value: f64 },
}

pub struct TelemetryEvent {
    pub edge_id: Uuid,
    pub run_id: Uuid,
    pub monotonic_ns: u64,
    pub wall_ns: u64,
    pub payload: EventPayload,
}
```

### 8.3 Sizing

Default: **1,000,000 events** (~200MB at ~200 bytes/event). At 10K tx/sec with ~3 events/tx, holds ~33s of data. Shipper should drain faster under normal conditions.

---

## 9. Telemetry Shipper

### 9.1 Batching

- Drain up to 1,000 events per batch
- Ship every 100ms or when batch full (whichever first)
- Optional zstd compression

### 9.2 Disconnect Behavior

1. Events accumulate in ring buffer
2. If ring fills, oldest dropped (with counter)
3. On reconnection, drain buffer in backfill mode
4. `dropped` counter included in batch metadata so hive knows about data loss

### 9.3 Shipper Metrics

| Metric | Type | Description |
| --- | --- | --- |
| `edge_telemetry_shipped` | Counter | Events successfully shipped |
| `edge_telemetry_dropped` | Counter | Events dropped (buffer overflow) |
| `edge_telemetry_buffer_util` | Gauge | Ring buffer fill percentage |
| `edge_telemetry_ship_latency_ms` | Histogram | gRPC round-trip per batch |

---

## 10. Run Execution Lifecycle

### 10.1 State Machine

```text
Idle → PreparingRun → AwaitingBarrier → Executing → Draining → Idle
                                            │
                                            ▼
                                         Aborting → Idle
```

### 10.2 Phase-by-Phase

1. **PreparingRun:** Init account pool (fetch nonces), connect feed relay, warm connection pools, estimate gas costs per template
2. **AwaitingBarrier:** Report `BarrierReady`; wait for `ReleaseBarrier` with `t_start`
3. **Executing:** At `t_start`, begin tx generation per rate limiter. On `PhaseTransition`, adjust rate/mode. Continue until final phase or `Abort`.
4. **Draining:** Stop generating new txs. Wait for in-flight receipts (up to `drain_timeout`, default 30s). Ship remaining telemetry. Report final status.
5. **Idle:** Ready for next assignment.

### 10.3 Phase Transition Handling

On `PhaseTransition` message:

1. Log `PhaseStarted` event
2. Update rate limiter configuration
3. Start/stop fault injection as specified
4. Continue generation with new parameters (no drain between phases)

---

## 11. Resource Self-Monitoring

### 11.1 Monitored Metrics

| Metric | Source | Alert Threshold |
| --- | --- | --- |
| CPU utilization | `/proc/stat` or `sysinfo` crate | >85% sustained |
| Memory RSS | `/proc/self/status` | >80% of available |
| Open file descriptors | `/proc/self/fd` count | >80% of ulimit |
| Tokio task count | `Handle::metrics()` | >10,000 |
| Event log fill | Ring buffer utilization | >80% |

### 11.2 Self-Protection

On self-saturation:

1. Emit `EdgeOverloaded` event
2. Reduce workload rate by 20%
3. Report `Degraded` in next heartbeat
4. Hive decides: redistribute or continue degraded

This prevents the measurement tool from becoming the bottleneck (PRD §11: "Metric ingestion should not become the first bottleneck").

---

## 12. Concurrency Model

### 12.1 Task Groups

| Task | Count | Description |
| --- | --- | --- |
| `hive_client` | 1 | gRPC bidirectional stream |
| `heartbeat` | 1 | Periodic heartbeat sender |
| `tx_generator` | N (default=4) | Parallel tx construction + submission |
| `feed_consumer` | 1 | WebSocket feed reader + correlator |
| `receipt_tracker` | 1 | Batched receipt poller |
| `telemetry_shipper` | 1 | Event log drain + gRPC shipping |
| `resource_monitor` | 1 | Self-monitoring (5s interval) |
| `fault_adapter` | 0–1 | Optional fault injection |

### 12.2 Synchronization

| Primitive | Use |
| --- | --- |
| `crossbeam::ArrayQueue` | Lock-free event log (MPSC ring) |
| `dashmap::DashMap` | Concurrent pending tx maps |
| `AtomicU64` | Nonce counters, rate limiter tokens |
| `tokio::sync::watch` | Phase config broadcast (1 writer, N readers) |
| `tokio::sync::oneshot` | Barrier release signal |
| `tokio::sync::mpsc` | Control messages from hive client to run executor |

### 12.3 No Mutexes in Hot Path

The tx generation → event log → telemetry shipper pipeline uses only lock-free and atomic operations. No `Mutex` or `RwLock` in the hot path. Minimizes measurement distortion.

---

## 13. Graceful Shutdown

**On SIGTERM/SIGINT:**

1. Stop tx generation immediately
2. Flush event log to shipper (best-effort, 5s timeout)
3. Send `DeregisterEdge` to hive
4. Close WebSocket + gRPC connections
5. Exit code 0

**On panic:**

- `std::panic::set_hook` logs with full backtrace via `tracing::error!`
- Exit code 1
- Hive detects via heartbeat timeout → marks edge `Offline`

---

## 14. Risks

### Risk: Feed decoding diverges from Nitro encoding

If Nitro changes `l2Msg` encoding, the correlator fails.

**Mitigation:** Log decode failures as `FeedDecodeError`. Pin expected encoding version in `gashammer-nitro`. Document Nitro version compatibility matrix.

### Risk: RPC rate limiting disrupts receipt tracking

**Mitigation:** Configurable poll interval and batch size. Back off on HTTP 429. Target dedicated nodes, not public endpoints (RFC-0001 §15).

### Risk: Account pool fund exhaustion

At sustained high gas rates, test accounts may deplete.

**Mitigation:** Preflight estimates total gas cost = `gas_rate × duration × avg_gas_price`. Warn if account balances are insufficient. Scenario compiler should validate funding requirements.

### Risk: Event log overflow under extreme throughput

At >50K tx/sec, the ring buffer may overflow.

**Mitigation:** Configurable buffer size. `dropped` counter exposed in metrics and run report. Shipper priority is higher than generation — if the system is saturated, the edge self-protects (§11.2).

### Risk: WebSocket feed relay becomes a bottleneck

Many edges connecting to a single relay may overload it.

**Mitigation:** Deploy multiple relays; hive assigns edges to relays. Edge config supports multiple relay addresses with failover.

---

## 15. Open Questions

1. Should the edge support hot-reloading of target endpoints mid-run (e.g., sequencer failover)?
2. What is the optimal `tx_generator` task count — should it auto-scale based on observed throughput vs. CPU?
3. Should feed correlation be optional (for environments without relay access)?
4. Should the edge persist its event log to disk for crash recovery, or is in-memory sufficient?
5. What is the maximum edge-to-hive latency before telemetry batching becomes problematic?

---

## 16. References

- PRD §9.1 (Edge Workers)
- PRD §9.2 (Hive Edge Architecture — Edge Responsibilities)
- PRD §10.3 (Workload Generation)
- PRD §10.4 (Measurement)
- RFC-0001 §3 (Transaction Submission)
- RFC-0001 §4 (Sequencer Feed)
- RFC-0001 §5 (RPC Gateway)
- RFC-0002 §3 (Core Dependencies)
- RFC-0003 §3 (Edge Registry)
- RFC-0003 §5 (Run Orchestration — Barriers)
- RFC-0003 §7 (gRPC Coordination Service)
