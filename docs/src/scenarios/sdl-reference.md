# SDL Reference

Complete reference for the GasHammer Scenario Definition Language.

## Top-Level Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Unique scenario name (alphanumeric, hyphens, underscores) |
| `description` | string | yes | Human-readable description |
| `version` | string | yes | Scenario version |

## Target Section

Connection parameters for the Nitro deployment under test.

```yaml
target:
  chain_id: 412346
  sequencer_rpc: "http://sequencer:8547"
  gateway_rpc: "http://gateway:8547"
  feed_relay: "ws://feed-relay:9642"
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `chain_id` | u64 | yes | Chain ID of the Nitro rollup |
| `sequencer_rpc` | URL | yes | Sequencer JSON-RPC endpoint |
| `gateway_rpc` | URL | no | Gateway JSON-RPC endpoint |
| `feed_relay` | URL | no | Feed relay WebSocket URL |

## Workload Section

Defines the transaction workload.

```yaml
workload:
  gas_rate:
    type: sustained | ramp | burst
    gas_per_sec: 5_000_000        # for sustained
    start_gas_per_sec: 1_000_000  # for ramp
    end_gas_per_sec: 10_000_000   # for ramp
    burst_gas: 50_000_000         # for burst
    burst_duration_secs: 5        # for burst
  seed: 42
  templates:
    - name: simple-transfer
      weight: 70
    - name: erc20-transfer
      weight: 30
```

### Gas Rate Types

| Type | Fields | Description |
|------|--------|-------------|
| `sustained` | `gas_per_sec` | Constant gas rate |
| `ramp` | `start_gas_per_sec`, `end_gas_per_sec` | Linear ramp between rates |
| `burst` | `burst_gas`, `burst_duration_secs` | Short burst followed by zero |

### Built-in Templates

| Template | Estimated Gas | Description |
|----------|--------------|-------------|
| `simple-transfer` | 21,000 | ETH value transfer |
| `erc20-transfer` | ~65,000 | ERC-20 token transfer |
| `erc20-approve` | ~46,000 | ERC-20 approval |
| `storage-write` | ~44,000+ | Write to storage slots |
| `compute-heavy` | configurable | CPU-intensive computation |

### Seed

The `seed` field (optional, defaults to `0`) seeds the deterministic PRNG. The same seed with the same scenario configuration produces the same transaction sequence.

## Phases Section

Phases define time segments within a run. Each phase can override the default gas rate.

```yaml
phases:
  - name: warmup
    duration_secs: 30
    gas_rate_override:
      type: sustained
      gas_per_sec: 1_000_000
  - name: steady-state
    duration_secs: 300
  - name: cooldown
    duration_secs: 30
    gas_rate_override:
      type: sustained
      gas_per_sec: 500_000
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `name` | string | yes | Phase name |
| `duration_secs` | u64 | yes | Phase duration in seconds (min: 1) |
| `gas_rate_override` | GasRate | no | Override the default gas rate for this phase |

## Oracle Section

Configure correctness checks.

```yaml
oracle:
  invariants:
    - balance-conservation
    - nonce-monotonicity
    - gas-accounting
  check_interval_secs: 10
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `invariants` | list | no | Which invariant checks to enable |
| `check_interval_secs` | u64 | no | How often to run live checks (default: 10) |

## Fault Schedule Section

Define controlled failures to inject during the run.

```yaml
fault_schedule:
  - at_secs: 60
    action: inject
    fault:
      type: latency
      target: sequencer-rpc
      latency_ms: 200
      jitter_ms: 50
  - at_secs: 120
    action: clear
    fault:
      type: latency
      target: sequencer-rpc
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `at_secs` | u64 | yes | Seconds from run start |
| `action` | `inject` or `clear` | yes | Whether to start or stop the fault |
| `fault.type` | string | yes | Fault type: `latency`, `packet-loss`, `feed-disconnect`, `partition` |
| `fault.target` | string | yes | Component to target |
| `fault.latency_ms` | u64 | varies | Added latency (for `latency` type) |
| `fault.jitter_ms` | u64 | no | Latency jitter |
| `fault.loss_pct` | f64 | varies | Packet loss percentage (for `packet-loss` type) |

## Edges Section

Control edge deployment.

```yaml
edges:
  count: 3
  accounts_per_edge: 100
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `count` | u32 | no | Number of edges (default: 1) |
| `accounts_per_edge` | u32 | no | Accounts per edge (default: 100) |
