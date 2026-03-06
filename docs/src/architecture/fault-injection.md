# Fault Injection

GasHammer injects controlled faults to test resilience under adverse conditions. The fault system is built around a pluggable adapter architecture with safety rails to prevent leaked faults.

Ref: RFC-0008.

## Fault Types

| Type | Adapter | Description |
|------|---------|-------------|
| `NetworkLatency` | netem | Added delay on network interface |
| `PacketLoss` | netem | Random packet drops |
| `BandwidthLimit` | netem | Throttled throughput |
| `NetworkJitter` | netem | Variable delay |
| `ConnectionReset` | iptables | TCP RST on matching connections |
| `PortBlock` | iptables | DROP on a target port |
| `FeedDisconnect` | (planned) | Kill WebSocket feed connection |
| `RpcSlowResponse` | (planned) | Inject RPC response delay |
| `RpcErrorInjection` | (planned) | Return errors from RPC |

## Adapter Architecture

Every adapter implements the `FaultAdapter` trait:

```rust
trait FaultAdapter: Send + Sync {
    fn name(&self) -> &str;
    fn supported_faults(&self) -> Vec<FaultType>;
    async fn preflight_check(&self) -> PreflightResult;
    async fn inject(&self, spec: FaultSpec) -> Result<FaultHandle, String>;
    async fn clear(&self, handle_id: Uuid) -> Result<(), String>;
    async fn clear_all(&self) -> Result<u32, String>;
}
```

**Lifecycle:**

1. `preflight_check()` --- verify prerequisites (binary exists, permissions).
2. `inject(spec)` --- apply the fault, return a `FaultHandle` with a UUID.
3. `clear(handle_id)` --- remove a specific fault by handle.
4. `clear_all()` --- remove all faults managed by this adapter.

### Netem Adapter

Wraps Linux `tc qdisc add dev <iface> root netem ...`. Requires `CAP_NET_ADMIN`.

**Parameters:**

| Parameter | Fault Types | Description |
|-----------|-------------|-------------|
| `interface` | all | Network interface (default: `eth0`) |
| `delay_ms` | Latency, Jitter | Delay in milliseconds |
| `jitter_ms` | Jitter | Jitter variation |
| `loss_pct` | PacketLoss | Loss percentage |
| `rate` | BandwidthLimit | Bandwidth cap (e.g., `1mbit`) |

**Cleanup:** `tc qdisc del dev <iface> root netem`.

### Iptables Adapter

Wraps `iptables -A INPUT ...`. Requires `CAP_NET_ADMIN`.

**Parameters:**

| Parameter | Fault Types | Description |
|-----------|-------------|-------------|
| `port` | PortBlock, ConnectionReset | Target port |
| `protocol` | all | `tcp` or `udp` (default: `tcp`) |

- `ConnectionReset` injects `-j REJECT --reject-with tcp-reset`.
- `PortBlock` injects `-j DROP`.

**Cleanup:** replays the same rule args with `-D` instead of `-A`.

## Fault Manager

`FaultManager` routes `inject()` calls to the correct adapter based on the fault type and tracks all active faults.

**Auto-clear:** When a `FaultSpec` includes a `duration`, the manager spawns a background task that calls `adapter.clear(handle_id)` after the duration elapses. The adapters are wrapped in `Arc<Vec<Box<dyn FaultAdapter>>>` to enable safe sharing across the spawn boundary.

**Safety invariant:** every injected fault is tracked by handle ID. `clear_all()` iterates all adapters and removes all active faults. This is called during shutdown to prevent fault leakage.

## Fault Timeline

A `FaultTimeline` is a sequence of scheduled fault events, defined in the scenario SDL:

```yaml
fault_schedule:
  - at_secs: 60
    action: inject
    fault:
      type: latency
      target: sequencer-rpc
      latency_ms: 200
  - at_secs: 120
    action: clear
    fault:
      type: latency
      target: sequencer-rpc
```

Each event specifies:

- `offset_ms` --- time from run start.
- `fault_name` --- human-readable label for correlation.
- `target_edges` --- `All`, `Region(name)`, or `Specific(vec![uuid])`.
- `action` --- `Inject(FaultSpec)` or `Clear { fault_name }`.

The timeline can restrict execution to specific environments via `allowed_environments` and `blocked_environments` to prevent accidental injection in production.

## Preflight Checks

Before a run starts, the fault manager calls `preflight_check()` on every adapter. The result reports:

```rust
struct PreflightResult {
    adapter_name: String,
    ready: bool,
    issues: Vec<String>,
}
```

If any required adapter is not ready, the run is blocked.
