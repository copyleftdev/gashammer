# RFC-0008: Fault Injection Framework

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0003 (Hive Control Plane)
- RFC-0004 (Edge Runtime)
- RFC-0010 (Scenario Definition Language)

## Summary

This RFC defines the fault injection framework — the subsystem that introduces controlled degradation into the test environment during a GasHammer run. Fault injection transforms GasHammer from a throughput benchmarker into a **resilience verification harness**. It answers: "What breaks first, and how does the system recover?"

---

## 1. Design Principles

1. **Faults are scheduled, not ad-hoc** — every fault is declared in the scenario definition with precise timing
2. **Faults are scoped** — each fault targets a specific component, network path, or resource
3. **Faults are reversible** — every injected fault has a corresponding clear operation
4. **Faults are observable** — injection and clearance are logged as telemetry events
5. **Faults must not escape** — cleanup runs on abort, timeout, and graceful shutdown

---

## 2. Fault Taxonomy

### 2.1 Network Faults

| Fault | Implementation | Target |
| --- | --- | --- |
| **Packet loss** | `tc netem loss` | Edge → target network path |
| **Latency injection** | `tc netem delay` | Edge → target network path |
| **Jitter** | `tc netem delay <base> <jitter>` | Edge → target network path |
| **Bandwidth clamp** | `tc tbf rate` | Edge → target network path |
| **Connection reset** | iptables `REJECT --reject-with tcp-reset` | Specific port/host |
| **DNS failure** | `/etc/hosts` manipulation or local DNS override | Edge DNS resolution |

### 2.2 Resource Faults

| Fault | Implementation | Target |
| --- | --- | --- |
| **CPU pressure** | `stress-ng --cpu` or cgroup CPU quota | Edge host or target host (if accessible) |
| **Memory pressure** | `stress-ng --vm` or cgroup memory limit | Edge host or target host |
| **Disk latency** | `dm-delay` or `fio` background load | Target host (if accessible) |
| **Disk full** | `fallocate` to fill a mount point | Target host (if accessible) |

### 2.3 Process Faults

| Fault | Implementation | Target |
| --- | --- | --- |
| **Process restart** | `kill -TERM` + wait + verify restart | Target node process (requires SSH/exec access) |
| **Process pause** | `kill -STOP` / `kill -CONT` | Target node process |
| **OOM kill** | cgroup memory limit to trigger OOM | Target node process |

### 2.4 Application-Level Faults

| Fault | Implementation | Target |
| --- | --- | --- |
| **RPC error injection** | Edge-local proxy returning errors for N% of requests | Edge → RPC path |
| **Feed disconnect** | Edge closes WebSocket deliberately | Edge → feed relay |
| **Slow response** | Edge-local proxy adding delay to responses | Edge → RPC path |

---

## 3. Adapter Architecture

### 3.1 Adapter Trait

```rust
pub trait FaultAdapter: Send + Sync {
    /// Human-readable adapter name
    fn name(&self) -> &str;

    /// Supported fault types
    fn supported_faults(&self) -> Vec<FaultType>;

    /// Inject a fault; returns a handle for clearing
    async fn inject(&self, fault: &FaultSpec) -> Result<FaultHandle, FaultError>;

    /// Clear a previously injected fault
    async fn clear(&self, handle: &FaultHandle) -> Result<(), FaultError>;

    /// Clear ALL faults managed by this adapter (emergency cleanup)
    async fn clear_all(&self) -> Result<(), FaultError>;

    /// Check if adapter prerequisites are met (e.g., tc installed, CAP_NET_ADMIN)
    async fn preflight_check(&self) -> Result<(), FaultError>;
}

pub struct FaultSpec {
    pub fault_type: FaultType,
    pub target: FaultTarget,
    pub parameters: FaultParams,
    pub duration: Option<Duration>,  // auto-clear after duration; None = manual clear
}

pub struct FaultHandle {
    pub id: Uuid,
    pub adapter: String,
    pub injected_at: Instant,
    pub spec: FaultSpec,
}
```

### 3.2 Built-in Adapters

| Adapter | Faults | Requirements |
| --- | --- | --- |
| `NetemAdapter` | Packet loss, latency, jitter, bandwidth | `tc` binary, `CAP_NET_ADMIN` |
| `IptablesAdapter` | Connection reset, port block | `iptables` binary, `CAP_NET_ADMIN` |
| `StressAdapter` | CPU pressure, memory pressure | `stress-ng` binary |
| `ProcessAdapter` | Kill, pause, restart | SSH access or `kubectl exec` to target |
| `ProxyAdapter` | RPC error injection, slow response | Local TCP proxy (built into edge) |
| `FeedFaultAdapter` | Deliberate feed disconnect/reconnect | Built into edge feed consumer |

### 3.3 Adapter Discovery

During edge registration, the edge reports its `supported_adapters` list (RFC-0003 §3.2). The scenario compiler validates that required fault adapters are available on assigned edges.

Preflight checks (RFC-0003 §5.2) verify adapter prerequisites:

- `NetemAdapter` checks for `tc` binary and `CAP_NET_ADMIN` capability
- `ProcessAdapter` checks SSH connectivity to target hosts
- Missing prerequisites → preflight failure → run aborted

---

## 4. Fault Scheduling

### 4.1 Scenario Definition

Faults are declared in the scenario YAML with precise timing:

```yaml
faults:
  - name: validator-packet-loss
    at: 12m              # inject at 12 minutes into the run
    type: packet_loss
    target: validator-group-b
    adapter: netem
    params:
      loss_percent: 5
      correlation: 25
    duration: 3m         # auto-clear after 3 minutes

  - name: sequencer-latency-spike
    at: 15m
    type: latency
    target: sequencer
    adapter: netem
    params:
      delay_ms: 200
      jitter_ms: 50
    clear_at: 18m        # explicit clear time

  - name: rpc-error-burst
    at: 20m
    type: rpc_error_injection
    adapter: proxy
    params:
      error_rate: 0.1    # 10% of RPC responses return error
      error_code: -32000
    duration: 2m

  - name: feed-disconnect
    at: 22m
    type: feed_disconnect
    adapter: feed_fault
    params:
      disconnect_duration_sec: 30
    edges: [us-west]     # only affect us-west edges
```

### 4.2 Fault Timeline

The hive's run orchestrator maintains a fault timeline:

```rust
pub struct FaultTimeline {
    pub events: Vec<FaultTimelineEvent>,
}

pub struct FaultTimelineEvent {
    pub offset: Duration,           // time since run start
    pub action: FaultAction,
    pub fault_name: String,
    pub target_edges: EdgeSelector, // specific edges, region, or all
}

pub enum FaultAction {
    Inject(FaultSpec),
    Clear(String),  // fault name to clear
}

pub enum EdgeSelector {
    All,
    Region(String),
    Specific(Vec<Uuid>),
}
```

The orchestrator sends `FaultInject` / `FaultClear` control messages to edges at the scheduled times.

---

## 5. Execution Model

### 5.1 Edge-Local Faults

Most faults execute locally on the edge:

```text
Hive                              Edge
  │                                 │
  │── FaultInject(spec) ──────────►│
  │                                 │── adapter.inject(spec)
  │                                 │── emit FaultInjected event
  │                                 │── start auto-clear timer (if duration set)
  │                                 │
  │     ... fault is active ...     │
  │                                 │
  │── FaultClear(name) ──────────►│  (or auto-clear timer fires)
  │                                 │── adapter.clear(handle)
  │                                 │── emit FaultCleared event
  │                                 │
```

### 5.2 Remote Faults

Process-level faults (kill, restart) target remote hosts. These execute via:

1. **SSH** — edge SSHs to target host and executes commands
2. **kubectl exec** — edge uses K8s API to exec into target pod
3. **API call** — edge calls a chaos engineering API (e.g., LitmusChaos, Chaos Mesh)

Remote fault adapters are optional and require explicit configuration:

```yaml
fault_adapters:
  process:
    type: ssh
    hosts:
      validator-group-b:
        address: "10.0.1.50"
        user: "gashammer"
        key_file: "/etc/gashammer/ssh/id_ed25519"
```

### 5.3 Proxy Adapter (Application-Level)

The edge can run a local TCP proxy that sits between its RPC client and the target:

```text
Edge Tx Generator → Local Proxy (:18545) → Target RPC (:8545)
```

The proxy intercepts HTTP responses and:

- Returns error responses at the configured rate
- Adds delay to responses
- Drops responses (simulating timeout)

This is useful for testing edge resilience and RPC failure handling without requiring network-level access.

---

## 6. Safety Mechanisms

### 6.1 Guaranteed Cleanup

Faults **must** be cleared under all circumstances:

| Trigger | Cleanup Action |
| --- | --- |
| Fault duration expires | Auto-clear timer fires |
| Run completes normally | Orchestrator clears all active faults |
| Run aborted | Orchestrator sends `ClearAllFaults` to all edges |
| Edge crashes | OS cleans up `tc` rules on process exit; SSH faults persist (see §6.3) |
| Hive crashes | Edges detect disconnect → `clear_all()` on all adapters |
| Edge SIGTERM | Shutdown handler calls `clear_all()` (RFC-0004 §13) |

### 6.2 Environment Allowlists

Fault injection is restricted to approved environments:

```yaml
fault_safety:
  allowed_environments:
    - "staging"
    - "perf-test"
  blocked_environments:
    - "production"
    - "mainnet"
  require_confirmation: true  # prompt before injecting faults
```

The hive validates the target environment against the allowlist during scenario compilation. Faults targeting blocked environments are rejected.

### 6.3 Persistent Fault Recovery

Some faults (SSH-based process kills, iptables rules) persist beyond edge process lifetime. To handle this:

1. Edge writes a **fault journal** to disk before injecting any persistent fault
2. On startup, edge reads the journal and clears any stale faults
3. Journal entries include: adapter name, fault spec, target, injection time
4. Journal is a simple append-only JSON file at `/var/lib/gashammer/fault-journal.json`

### 6.4 Dry Run Mode

Scenarios can be validated without actually injecting faults:

```yaml
faults:
  dry_run: true  # log fault events but don't execute adapters
```

---

## 7. Fault Metrics

| Metric | Type | Description |
| --- | --- | --- |
| `fault_active` | Gauge | Number of currently active faults |
| `fault_injected_total` | Counter | Total faults injected |
| `fault_cleared_total` | Counter | Total faults cleared |
| `fault_inject_errors` | Counter | Failed injection attempts |
| `fault_clear_errors` | Counter | Failed clear attempts |
| `fault_duration_seconds` | Histogram | Duration of active faults |

---

## 8. Composability with Workload Phases

Faults interact with scenario phases (RFC-0003 §5.4):

- A fault can span multiple phases
- Phase transitions do not automatically clear faults
- The `recover` phase typically has no active faults — measuring recovery behavior
- Fault timing is specified as absolute offset from run start, not relative to phase boundaries

This allows scenarios like:

```
Phase: ramp (0–5m)     → no faults, baseline measurement
Phase: sustain (5–20m)  → inject packet loss at 12m, clear at 15m
Phase: recover (20–25m) → no faults, measure backlog recovery
```

---

## 9. Risks

### Risk: tc/netem rules affect GasHammer's own telemetry traffic

Network faults applied broadly may degrade the edge↔hive gRPC connection.

**Mitigation:** `NetemAdapter` applies rules only to traffic targeting the SUT's IP/port, not the hive's IP/port. Use `tc filter` with destination IP matching.

### Risk: Fault injection requires elevated privileges

`CAP_NET_ADMIN`, root SSH access, etc. are not always available.

**Mitigation:** Faults are adapter-based and optional. Scenarios validate adapter availability at preflight. Missing adapters → those fault steps are skipped with a warning (or aborted if `strict` mode).

### Risk: Faults leak into shared environments

In multi-tenant staging environments, fault injection could affect other users.

**Mitigation:** Environment allowlists (§6.2). Scoped `tc` rules targeting specific IPs/ports. `ProcessAdapter` targets specific process names/PIDs. Dry-run mode for validation.

### Risk: Edge crash leaves network faults in place

If the edge process is killed (SIGKILL), shutdown handlers don't run.

**Mitigation:** Fault journal (§6.3). `tc` rules are tied to network namespaces — if the edge runs in a container, namespace cleanup on container restart clears rules. For VM deployments, the fault journal handles recovery.

---

## 10. Open Questions

1. Should GasHammer integrate with existing chaos engineering platforms (Chaos Mesh, LitmusChaos) instead of building its own adapters?
2. Should faults support probabilistic timing (e.g., "inject sometime between 10m and 15m")?
3. Should the `ProxyAdapter` support full HTTP request/response manipulation (like a mock server)?
4. Is the fault journal sufficient for crash recovery, or should we use a more robust mechanism (e.g., systemd drop-in)?
5. Should faults be injectable via the REST API for ad-hoc testing outside of scenarios?

---

## 11. References

- PRD §10.6 (Fault Injection)
- PRD §11 (Security — scoped permissions, environment allowlists)
- PRD §20 (Risk: fault injectors become unsafe in shared environments)
- RFC-0003 §5.4 (Phase Transitions)
- RFC-0004 §2 (Fault Adapter in edge architecture)
- RFC-0004 §13 (Graceful Shutdown — clear_all)
