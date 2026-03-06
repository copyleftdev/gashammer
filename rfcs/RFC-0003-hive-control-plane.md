# RFC-0003: Hive Control Plane

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0002 (Technology Stack & Dependencies)

## Summary

This RFC defines the architecture of the **Hive** — GasHammer's central control plane. The hive is responsible for edge fleet management, scenario compilation and distribution, barrier-synchronized run execution, telemetry aggregation, and artifact lifecycle. It exposes a REST API for operators/CI and a gRPC coordination plane for edges.

---

## 1. Responsibilities

The hive is the single coordination point for all GasHammer activity:

1. **Edge Registry** — register, authenticate, health-check, and deregister edge workers
2. **Scenario Lifecycle** — accept, validate, compile, and partition scenario definitions
3. **Run Orchestration** — schedule runs, synchronize start barriers, manage phase transitions, handle abort/drain
4. **Telemetry Aggregation** — receive event streams from edges, merge timelines, store raw + aggregated data
5. **Artifact Management** — version and store run inputs, outputs, and reproducibility bundles
6. **API Surface** — REST for humans/CI; gRPC for edges

---

## 2. Process Architecture

```
┌──────────────────────────────────────────────────────────┐
│                    gashammer-hive                         │
│                                                          │
│  ┌─────────────┐   ┌──────────────┐   ┌──────────────┐  │
│  │  REST API   │   │  gRPC Server │   │  Background  │  │
│  │  (axum)     │   │  (tonic)     │   │  Workers     │  │
│  │  :8080      │   │  :9090       │   │              │  │
│  └──────┬──────┘   └──────┬───────┘   └──────┬───────┘  │
│         │                 │                   │          │
│  ┌──────┴─────────────────┴───────────────────┴───────┐  │
│  │                  Core Services                     │  │
│  │                                                    │  │
│  │  ┌────────────┐  ┌─────────────┐  ┌────────────┐  │  │
│  │  │ Edge       │  │ Run         │  │ Scenario   │  │  │
│  │  │ Registry   │  │ Orchestrator│  │ Compiler   │  │  │
│  │  └────────────┘  └─────────────┘  └────────────┘  │  │
│  │                                                    │  │
│  │  ┌────────────┐  ┌─────────────┐  ┌────────────┐  │  │
│  │  │ Barrier    │  │ Telemetry   │  │ Artifact   │  │  │
│  │  │ Manager    │  │ Aggregator  │  │ Store      │  │  │
│  │  └────────────┘  └─────────────┘  └────────────┘  │  │
│  └────────────────────────────────────────────────────┘  │
│                          │                               │
│  ┌───────────────────────┴────────────────────────────┐  │
│  │              Storage Layer                         │  │
│  │  PostgreSQL (metadata)    S3 (artifacts)           │  │
│  └────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

The hive runs as a single process with multiple async task groups. In MVP, it is a single-instance deployment. Horizontal scaling (leader election, sharded edge assignment) is deferred.

---

## 3. Edge Registry

### 3.1 Data Model

```rust
pub struct EdgeRecord {
    pub edge_id: Uuid,
    pub region: String,                    // e.g., "us-west-2"
    pub capacity_class: CapacityClass,     // Small, Medium, Large, Custom
    pub software_version: semver::Version,
    pub supported_adapters: Vec<String>,   // e.g., ["nitro-rpc", "fault-netem"]
    pub instance_profile: InstanceProfile, // CPU cores, memory, network class
    pub status: EdgeStatus,
    pub last_heartbeat: Instant,
    pub registered_at: DateTime<Utc>,
    pub tls_fingerprint: String,           // certificate fingerprint for identity
}

pub enum EdgeStatus {
    Registering,
    Healthy,
    Degraded { reason: String },
    Draining,
    Offline,
}

pub enum CapacityClass {
    Small,   // ≤4 cores, ≤8GB RAM
    Medium,  // ≤16 cores, ≤32GB RAM
    Large,   // ≤64 cores, ≤128GB RAM
    Custom { cores: u32, memory_mb: u64 },
}
```

### 3.2 Registration Flow

```
Edge                                  Hive
  │                                     │
  │──── RegisterEdge(bootstrap_token) ──►│
  │                                     │── validate token
  │                                     │── verify mTLS cert
  │                                     │── create EdgeRecord
  │◄──── RegisterEdgeResponse(edge_id) ─│
  │                                     │
  │──── Heartbeat (every 5s) ──────────►│
  │◄──── HeartbeatAck ─────────────────│
  │                                     │
```

### 3.3 Health Tracking

- **Heartbeat interval:** 5 seconds (configurable)
- **Heartbeat timeout:** 15 seconds (3 missed beats → `Degraded`)
- **Offline threshold:** 30 seconds (6 missed beats → `Offline`)
- **Heartbeat payload:** edge CPU/memory utilization, active connection count, current workload status

### 3.4 Edge Lifecycle States

```
  Registering ──► Healthy ──► Draining ──► Offline
                    │   ▲         │
                    ▼   │         │
                 Degraded ────────┘
```

- **Healthy → Degraded:** missed heartbeats or self-reported degradation
- **Degraded → Healthy:** heartbeats resume within threshold
- **Healthy/Degraded → Draining:** operator or scenario requests graceful removal
- **Draining → Offline:** all in-flight work completes or times out
- **Any → Offline:** extended heartbeat loss

### 3.5 Edge Eviction During a Run

When an edge goes `Offline` during an active run:

1. Hive marks the edge's workload slice as `orphaned`
2. If scenario policy is `strict`: abort the run
3. If scenario policy is `best-effort`: redistribute orphaned slice to healthy edges in the same region (if available), otherwise continue with reduced coverage
4. Log the event with full context for the run report

---

## 4. Scenario Lifecycle

### 4.1 States

```
Submitted ──► Validating ──► Compiled ──► Scheduled ──► Running ──► Completing ──► Done
                 │                           │            │              │
                 ▼                           ▼            ▼              ▼
              Invalid                    Cancelled     Aborted       Failed
```

### 4.2 Submission

Scenarios are submitted via REST API as YAML payloads (see RFC-0010 for schema). The hive:

1. Parses YAML
2. Validates against JSON Schema
3. Checks that referenced edges/regions exist and are healthy
4. Compiles workload into per-edge slices (see §4.3)
5. Stores the compiled scenario as an immutable artifact

### 4.3 Compilation (Scenario → Edge Slices)

The scenario compiler transforms a high-level scenario definition into concrete per-edge work assignments:

```rust
pub struct CompiledScenario {
    pub scenario_id: Uuid,
    pub run_id: Uuid,
    pub seed: u64,
    pub duration: Duration,
    pub phases: Vec<Phase>,
    pub edge_assignments: Vec<EdgeAssignment>,
    pub thresholds: ThresholdSet,
    pub correctness_checks: Vec<OracleConfig>,
}

pub struct EdgeAssignment {
    pub edge_id: Uuid,
    pub region: String,
    pub workload_slice: WorkloadSlice,
    pub target_config: TargetConfig,   // RPC endpoints, feed relay, chain ID
    pub fault_schedule: Vec<FaultStep>,
}

pub struct WorkloadSlice {
    pub tx_templates: Vec<WeightedTemplate>,
    pub gas_rate_limit: GasRate,       // gas/sec target for this edge
    pub account_pool: AccountPool,     // pre-funded accounts assigned to this edge
    pub seed: u64,                     // deterministic sub-seed for this edge
    pub burst_config: Option<BurstConfig>,
}
```

**Partitioning rules:**

- Regional percentage allocation from scenario → mapped to edges in that region
- Gas budget split proportionally across edges in a region
- Account pools are disjoint — no two edges share accounts (prevents nonce collisions)
- Seeds are derived deterministically from the master seed + edge index

### 4.4 Artifact Immutability

Once compiled, the scenario artifact is written to S3 and its content hash is recorded. The compiled artifact includes:

- Original YAML source
- Resolved JSON Schema version
- Compiled per-edge slices
- Master seed
- Software versions (hive, edge, scenario compiler)
- Target environment snapshot (chain ID, endpoint URLs, block height at compile time)

This bundle is the **reproducibility record** referenced in PRD §14 (Release Gate Report).

---

## 5. Run Orchestration

### 5.1 Run Lifecycle

```rust
pub enum RunState {
    Preflight,   // health checks, skew estimation, credential validation
    Barrier,     // waiting for all edges to acknowledge readiness
    Running {
        phase: usize,
        phase_name: String,
    },
    Recovering,  // post-workload; waiting for drain + oracle checks
    Completing,  // generating reports, storing artifacts
    Done { outcome: RunOutcome },
    Aborted { reason: String },
}

pub enum RunOutcome {
    Pass,
    Fail { violations: Vec<ThresholdViolation> },
    Inconclusive { reason: String },
}
```

### 5.2 Preflight Checks

Before a run starts, the hive performs:

1. **Edge health:** All assigned edges are `Healthy`
2. **Clock skew estimation:** Each edge reports its wall clock; hive computes pairwise skew. Warn if >50ms; abort if >250ms (configurable)
3. **Target reachability:** Each edge confirms it can reach its assigned sequencer RPC, RPC gateway, and feed relay
4. **Account funding:** Each edge confirms its assigned accounts have sufficient balance for the planned workload
5. **Software version compatibility:** Edge version matches or exceeds the minimum required by the compiled scenario

If any preflight check fails, the run transitions to `Aborted` with a diagnostic report.

### 5.3 Barrier Synchronization

The barrier is the mechanism that ensures all edges start generating load simultaneously:

```
Hive                          Edge-1        Edge-2        Edge-3
  │                             │             │             │
  │── PrepareBarrier(run_id) ──►│             │             │
  │── PrepareBarrier(run_id) ──►─────────────►│             │
  │── PrepareBarrier(run_id) ──►─────────────►─────────────►│
  │                             │             │             │
  │◄── BarrierReady ───────────│             │             │
  │◄── BarrierReady ───────────│─────────────│             │
  │◄── BarrierReady ───────────│─────────────│─────────────│
  │                             │             │             │
  │  (all edges ready)          │             │             │
  │                             │             │             │
  │── ReleaseBarrier(t_start) ─►│             │             │
  │── ReleaseBarrier(t_start) ─►─────────────►│             │
  │── ReleaseBarrier(t_start) ─►─────────────►─────────────►│
  │                             │             │             │
  │                         [BEGIN WORKLOAD]                 │
```

- `t_start` is a wall-clock timestamp slightly in the future (e.g., now + 500ms) to absorb message delivery jitter
- Edges begin workload generation at `t_start` using their local clocks
- NFR target: synchronization skew <250ms across healthy edges (PRD §11)

**Barrier timeout:** If not all edges report `BarrierReady` within 30 seconds, abort the run.

### 5.4 Phase Transitions

Scenarios define phases (e.g., ramp → sustain → recover). The hive manages transitions:

1. Hive tracks elapsed time since `t_start`
2. When a phase boundary is reached, hive sends `PhaseTransition` message to all edges
3. Edges adjust their workload parameters (rate, burst mode, fault injection) accordingly
4. Phase transitions are logged as timeline events in the run record

### 5.5 Abort and Drain

**Abort** (immediate stop):

- Hive sends `Abort` to all edges
- Edges stop workload generation immediately
- In-flight transactions are recorded but not waited on
- Run state → `Aborted`

**Drain** (graceful stop):

- Hive sends `Drain` to specific edges or all edges
- Edges stop generating new transactions but wait for in-flight receipts (up to configurable timeout)
- Edge reports final metrics and transitions to idle
- Used for single-edge removal without aborting the full run

---

## 6. REST API

### 6.1 Routes

All routes prefixed with `/api/v1`.

#### Scenarios

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/scenarios` | Submit a new scenario (YAML body) |
| `GET` | `/scenarios` | List scenarios (paginated) |
| `GET` | `/scenarios/{id}` | Get scenario details |
| `POST` | `/scenarios/{id}/validate` | Validate without scheduling |
| `DELETE` | `/scenarios/{id}` | Delete a scenario (if no active runs) |

#### Runs

| Method | Path | Description |
| --- | --- | --- |
| `POST` | `/runs` | Start a run from a compiled scenario |
| `GET` | `/runs` | List runs (paginated, filterable by status) |
| `GET` | `/runs/{id}` | Get run status + summary |
| `GET` | `/runs/{id}/timeline` | Get phase-by-phase timeline |
| `POST` | `/runs/{id}/abort` | Abort an active run |
| `GET` | `/runs/{id}/report` | Get the final report |
| `GET` | `/runs/{id}/artifacts` | List artifacts for the run |
| `GET` | `/runs/{id}/artifacts/{name}` | Download a specific artifact |

#### Edges

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/edges` | List registered edges |
| `GET` | `/edges/{id}` | Get edge details + health |
| `POST` | `/edges/{id}/drain` | Drain an edge (graceful removal) |
| `DELETE` | `/edges/{id}` | Force-deregister an edge |

#### System

| Method | Path | Description |
| --- | --- | --- |
| `GET` | `/health` | Hive health check |
| `GET` | `/metrics` | Prometheus metrics endpoint |
| `GET` | `/version` | Hive software version |

### 6.2 Request/Response Format

- **Content-Type:** `application/json` for all responses; `application/yaml` or `application/json` for scenario submission
- **Pagination:** cursor-based (`?cursor=<opaque>&limit=50`)
- **Error format:**

```json
{
  "error": {
    "code": "SCENARIO_VALIDATION_FAILED",
    "message": "Workload profile 'defi-heavy' references undefined template",
    "details": {
      "field": "workload.profiles[2].name",
      "value": "defi-heavy"
    }
  }
}
```

### 6.3 Authentication

MVP: API key in `Authorization: Bearer <key>` header. The key is configured in hive config.

Future: OAuth2/OIDC for multi-tenant access control.

---

## 7. gRPC Coordination Service

### 7.1 Service Definitions

```protobuf
syntax = "proto3";
package gashammer.v1;

service HiveCoordinator {
  // Edge lifecycle
  rpc RegisterEdge(RegisterEdgeRequest) returns (RegisterEdgeResponse);
  rpc Heartbeat(stream HeartbeatMessage) returns (stream HeartbeatAck);
  rpc DeregisterEdge(DeregisterEdgeRequest) returns (DeregisterEdgeResponse);

  // Run coordination
  rpc PrepareRun(PrepareRunRequest) returns (PrepareRunResponse);
  rpc BarrierReady(BarrierReadyRequest) returns (BarrierReadyResponse);
  rpc StreamControl(stream EdgeStatusUpdate) returns (stream ControlMessage);

  // Telemetry
  rpc StreamEvents(stream TelemetryEvent) returns (StreamEventsResponse);
  rpc StreamMetrics(stream MetricBatch) returns (StreamMetricsResponse);
}
```

### 7.2 Key Message Types

```protobuf
message RegisterEdgeRequest {
  string bootstrap_token = 1;
  string region = 2;
  string capacity_class = 3;
  string software_version = 4;
  repeated string supported_adapters = 5;
  InstanceProfile instance_profile = 6;
}

message ControlMessage {
  oneof message {
    PrepareBarrier prepare_barrier = 1;
    ReleaseBarrier release_barrier = 2;
    PhaseTransition phase_transition = 3;
    AbortRun abort_run = 4;
    DrainEdge drain_edge = 5;
    WorkloadSlice workload_slice = 6;
  }
}

message ReleaseBarrier {
  string run_id = 1;
  int64 start_timestamp_ns = 2;  // wall clock nanoseconds
}

message PhaseTransition {
  string run_id = 1;
  uint32 phase_index = 2;
  string phase_name = 3;
  int64 transition_timestamp_ns = 4;
}

message HeartbeatMessage {
  string edge_id = 1;
  int64 timestamp_ns = 2;
  float cpu_utilization = 3;
  float memory_utilization = 4;
  uint32 active_connections = 5;
  string current_run_id = 6;  // empty if idle
}

message TelemetryEvent {
  string edge_id = 1;
  string run_id = 2;
  int64 monotonic_ns = 3;
  int64 wall_ns = 4;
  oneof event {
    TxSubmitted tx_submitted = 10;
    TxAccepted tx_accepted = 11;
    TxIncluded tx_included = 12;
    TxFailed tx_failed = 13;
    FeedMessage feed_message = 14;
    FeedDisconnect feed_disconnect = 15;
    FaultInjected fault_injected = 16;
    PhaseStarted phase_started = 17;
  }
}
```

### 7.3 Streaming Model

- **Heartbeat:** Bidirectional stream. Edge sends heartbeats; hive sends acks (which may carry piggyback control info).
- **StreamControl:** Bidirectional. Edge sends status updates; hive sends control messages (barriers, phase transitions, abort).
- **StreamEvents / StreamMetrics:** Client-streaming. Edge sends telemetry; hive acks periodically. Buffered on the edge side if the stream is temporarily unavailable.

---

## 8. Data Model (PostgreSQL)

### 8.1 Core Tables

```sql
-- Edge registry
CREATE TABLE edges (
    edge_id       UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    region        TEXT NOT NULL,
    capacity_class TEXT NOT NULL,
    software_version TEXT NOT NULL,
    instance_profile JSONB NOT NULL,
    status        TEXT NOT NULL DEFAULT 'registering',
    tls_fingerprint TEXT NOT NULL,
    last_heartbeat TIMESTAMPTZ,
    registered_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    metadata      JSONB DEFAULT '{}'
);

-- Scenario definitions
CREATE TABLE scenarios (
    scenario_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name          TEXT NOT NULL,
    source_yaml   TEXT NOT NULL,
    compiled      JSONB,
    schema_version TEXT NOT NULL,
    content_hash  TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_by    TEXT
);

-- Runs
CREATE TABLE runs (
    run_id        UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    scenario_id   UUID NOT NULL REFERENCES scenarios(scenario_id),
    state         TEXT NOT NULL DEFAULT 'preflight',
    outcome       TEXT,
    seed          BIGINT NOT NULL,
    started_at    TIMESTAMPTZ,
    completed_at  TIMESTAMPTZ,
    config        JSONB NOT NULL,
    metadata      JSONB DEFAULT '{}'
);

-- Per-run edge assignments
CREATE TABLE run_edges (
    run_id        UUID NOT NULL REFERENCES runs(run_id),
    edge_id       UUID NOT NULL REFERENCES edges(edge_id),
    region        TEXT NOT NULL,
    workload_slice JSONB NOT NULL,
    status        TEXT NOT NULL DEFAULT 'assigned',
    PRIMARY KEY (run_id, edge_id)
);

-- Run phases
CREATE TABLE run_phases (
    run_id        UUID NOT NULL REFERENCES runs(run_id),
    phase_index   INT NOT NULL,
    phase_name    TEXT NOT NULL,
    started_at    TIMESTAMPTZ,
    ended_at      TIMESTAMPTZ,
    PRIMARY KEY (run_id, phase_index)
);

-- Artifacts
CREATE TABLE artifacts (
    artifact_id   UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id        UUID NOT NULL REFERENCES runs(run_id),
    name          TEXT NOT NULL,
    content_type  TEXT NOT NULL,
    s3_key        TEXT NOT NULL,
    size_bytes    BIGINT NOT NULL,
    content_hash  TEXT NOT NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_runs_scenario ON runs(scenario_id);
CREATE INDEX idx_runs_state ON runs(state);
CREATE INDEX idx_artifacts_run ON artifacts(run_id);
CREATE INDEX idx_edges_status ON edges(status);
CREATE INDEX idx_edges_region ON edges(region);
```

### 8.2 Migration Strategy

Use `sqlx` compile-time checked migrations in `migrations/` directory. Sequential, forward-only migrations for MVP.

---

## 9. Concurrency Model

### 9.1 Tokio Task Groups

The hive process runs on a multi-threaded tokio runtime with the following task groups:

| Task Group | Responsibility | Failure Mode |
| --- | --- | --- |
| `rest_server` | axum HTTP server | Restart on panic; log error |
| `grpc_server` | tonic gRPC server | Restart on panic; log error |
| `heartbeat_monitor` | Periodic sweep of edge health | Log + alert; edges go stale |
| `run_orchestrator` | Per-run state machine (one task per active run) | Run-scoped; failure aborts that run |
| `telemetry_writer` | Batched writes of incoming telemetry to storage | Backpressure via bounded channel; drop oldest on overflow with metric |
| `artifact_gc` | Periodic cleanup of expired artifacts | Best-effort; log failures |

### 9.2 Shared State

Core services share state via `Arc<RwLock<T>>` or actor-pattern channels:

- **Edge registry:** `Arc<RwLock<HashMap<Uuid, EdgeRecord>>>` + PostgreSQL for durability
- **Active runs:** One `tokio::sync::mpsc` channel per run for control messages
- **Telemetry buffer:** Bounded `tokio::sync::mpsc` channel (configurable depth, default 100K events)

### 9.3 Backpressure

If telemetry ingestion exceeds write throughput:

1. Bounded channel fills
2. Oldest events are dropped (lossy channel)
3. A `telemetry_dropped` counter is incremented
4. The count is included in the run report as a data quality indicator

This is preferable to blocking edges, which would distort their workload timing.

---

## 10. Scaling Considerations (Post-MVP)

### 10.1 Single-Leader Limitation

MVP runs a single hive instance. This limits:

- Edge fleet to ~100 edges (PRD §11 NFR)
- Concurrent runs to ~10 (bounded by orchestrator task count and DB write throughput)
- Telemetry ingestion to what one process can handle

### 10.2 Future Scaling Path

- **Leader election** (via PostgreSQL advisory locks or etcd) for hive HA
- **Telemetry offload** to Kafka/NATS for decoupled ingestion
- **Read replicas** for API query scaling
- **Sharded edge assignment** — edges assigned to hive shards by region

These are documented here for architectural awareness but are **not in MVP scope**.

---

## 11. Risks

### Risk: Single point of failure

The hive is a single process. If it crashes mid-run, the run is lost.

**Mitigation:** Edges buffer telemetry locally. On hive restart, edges reconnect and replay buffered data. Run state is persisted in PostgreSQL; orchestrator can resume from last known phase. Full implementation of resume is post-MVP; MVP records the failure and requires manual re-run.

### Risk: Clock skew between hive and edges

Barrier synchronization depends on wall clocks.

**Mitigation:** Preflight skew estimation (§5.2). The `t_start` offset (500ms default) absorbs typical NTP jitter. Edges use monotonic clocks for all latency measurements; wall clocks are only used for barrier coordination and cross-region comparison.

### Risk: gRPC stream drops under network partition

A network partition between hive and an edge drops the gRPC stream.

**Mitigation:** Edges detect stream loss and buffer locally. Reconnection with exponential backoff. Hive treats the edge as `Degraded` after heartbeat timeout. Telemetry is replayed on reconnection (idempotent writes keyed by edge_id + monotonic timestamp).

### Risk: PostgreSQL becomes a bottleneck

High-frequency telemetry writes may saturate the database.

**Mitigation:** Telemetry is batched (write every 1s or every 10K events). Raw telemetry goes to S3 as Parquet; PostgreSQL stores only aggregated summaries. This separation is designed in from the start.

---

## 12. Open Questions

1. Should the hive support multiple concurrent runs against different target environments, or enforce single-run-at-a-time for simplicity in MVP?
2. What is the right telemetry batching interval — 1s or 5s? Lower latency vs. write amplification.
3. Should the REST API support SSE (Server-Sent Events) for live run progress, or is polling sufficient for MVP?
4. How should the hive handle edge version skew — reject old edges or maintain backward compatibility for N-1 versions?
5. Should barrier synchronization use a two-phase commit (prepare + release) or a simpler countdown approach?

---

## 13. References

- PRD §9.1 (Hive Control Plane components)
- PRD §9.2 (Hive Edge Architecture)
- PRD §10.2 (Distributed Edge Management)
- PRD §11 (Non-Functional Requirements)
- RFC-0001 §14 (Integration Boundaries — mTLS, auth)
- RFC-0002 §4 (Wire Protocol: gRPC over mTLS)
- RFC-0002 §5 (External API: REST)
- RFC-0002 §8 (Deployment Model)
