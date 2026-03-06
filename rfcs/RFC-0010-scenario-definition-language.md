# RFC-0010: Scenario Definition Language

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0003 (Hive Control Plane)
- RFC-0005 (Workload Engine & Gas Modeling)
- RFC-0007 (Correctness Oracle)
- RFC-0008 (Fault Injection Framework)

## Summary

This RFC defines the **Scenario Definition Language (SDL)** — the declarative YAML-based format in which engineers author GasHammer test scenarios. The SDL is the primary user-facing interface of the system. A well-designed SDL makes the difference between a tool that gets adopted and one that gathers dust. Every scenario must be self-contained, reproducible, version-controlled, and human-readable.

---

## 1. Design Principles

1. **Scenarios are code** (PRD §6.4) — they live in version control, are reviewed in PRs, and are diffable
2. **Declarative, not imperative** — describe *what* to test, not *how* to execute it
3. **Self-contained** — a scenario file + its referenced ABIs is sufficient to reproduce a run
4. **Validated before execution** — the compiler rejects invalid scenarios with clear error messages
5. **Progressive complexity** — simple scenarios should be simple to write; advanced features are opt-in

---

## 2. Schema Overview

```yaml
# gashammer scenario definition
version: "1.0"

scenario:
  name: string                    # human-readable name
  description: string             # optional
  tags: [string]                  # for filtering/searching
  seed: uint64                    # master PRNG seed for reproducibility

target:
  chain_id: uint64                # Arbitrum chain ID
  chain_type: rollup | anytrust   # determines observation strategy
  sequencer_rpc: url              # sequencer endpoint
  gateway_rpc: url                # RPC gateway endpoint
  feed_relay: url                 # WebSocket feed relay
  l1_rpc: url                     # L1 Ethereum node (for batch/validator monitoring)
  prometheus_endpoints: [...]     # optional target node metrics

duration: duration_string         # total run duration (e.g., "30m")

phases:
  - name: string
    duration: duration_string
    workload: workload_config     # override workload for this phase

edges:
  region_allocation: map          # region → percentage
  edge_policy: strict | best_effort
  min_edges_per_region: uint      # optional minimum

workload:
  target_gas_rate: gas_rate_config
  profiles: [weighted_template]
  contention: contention_config   # optional
  account_pool: account_config

faults: [fault_config]            # optional

correctness:
  checks: [check_config]
  thresholds: threshold_config

regression:                       # optional
  baseline: string
  thresholds: regression_thresholds

templates:                        # optional custom templates
  name: template_config

incident_replay:                  # optional
  incident_id: string
  expected_symptoms: [symptom]
```

---

## 3. Section Specifications

### 3.1 `version`

Schema version string. Currently `"1.0"`. The scenario compiler rejects unknown versions.

### 3.2 `scenario`

```yaml
scenario:
  name: "sequencer-global-burst"
  description: "Multi-region burst test targeting sequencer saturation point"
  tags: ["sequencer", "burst", "multi-region", "release-gate"]
  seed: 42
```

- `name` — **required**. Used in reports and artifact paths.
- `seed` — **required**. If omitted, compiler generates one and logs it. Explicit seed recommended for reproducibility.

### 3.3 `target`

```yaml
target:
  chain_id: 421614
  chain_type: rollup
  sequencer_rpc: "https://sequencer.staging.internal/rpc"
  gateway_rpc: "https://rpc.staging.internal/rpc"
  feed_relay: "ws://feed-relay.staging.internal:9642"
  l1_rpc: "https://l1-rpc.staging.internal/rpc"
  prometheus_endpoints:
    - name: sequencer
      url: "http://sequencer.staging.internal:6070/debug/metrics"
    - name: validator
      url: "http://validator.staging.internal:6070/debug/metrics"
```

- `chain_id` — **required**. Validated against the actual chain at preflight.
- `chain_type` — **required**. `rollup` (L1 data posting via blobs/calldata) or `anytrust` (DAC).
- `sequencer_rpc` — **required**. Must support `eth_sendRawTransaction`.
- `gateway_rpc` — **required**. General-purpose JSON-RPC.
- `feed_relay` — **required**. WebSocket URL for sequencer feed.
- `l1_rpc` — **required** for batch/validator monitoring. L1 Ethereum node.
- `prometheus_endpoints` — optional. Enriches reports with infra metrics.

### 3.4 `duration`

Total run duration. The sum of phase durations must equal this value.

```yaml
duration: "30m"
```

Duration strings: `"30s"`, `"5m"`, `"1h30m"`, etc.

### 3.5 `phases`

```yaml
phases:
  - name: ramp
    duration: "5m"
    workload:
      target_gas_rate:
        mode: ramped
        start: 10000000
        end: 100000000

  - name: sustain
    duration: "20m"
    # inherits top-level workload config

  - name: recover
    duration: "5m"
    workload:
      target_gas_rate:
        mode: sustained
        value: 0  # no new submissions; measure recovery
```

- Phases execute sequentially.
- If a phase omits `workload`, it inherits the top-level `workload` config.
- Phase `workload` fields are merged with (not replaced by) the top-level config. Explicit fields override; omitted fields inherit.

### 3.6 `edges`

```yaml
edges:
  region_allocation:
    us-west-2: 30
    us-east-1: 20
    eu-central-1: 25
    ap-southeast-1: 25
  edge_policy: best_effort
  min_edges_per_region: 1
```

- `region_allocation` — **required**. Percentages (must sum to 100).
- `edge_policy` — `strict` (abort if any region has no healthy edges) or `best_effort` (continue with available edges, warn about gaps). Default: `best_effort`.
- `min_edges_per_region` — optional minimum. Preflight fails if any region has fewer healthy edges.

### 3.7 `workload`

```yaml
workload:
  target_gas_rate:
    mode: sustained
    value: 50000000  # 50M gas/sec

  profiles:
    - template: simple-transfer
      weight: 40
    - template: erc20-transfer
      weight: 25
    - template: compute-heavy
      weight: 20
    - template: calldata-heavy
      weight: 15

  contention:
    mode: distributed  # or hot_account, hot_slot, nonce_race, spam_flood

  account_pool:
    count: 10000
    funding: prefunded
    # keys injected via secrets, not specified here
```

- `target_gas_rate` — see RFC-0005 §3.2 for `GasRate` variants.
- `profiles` — template name + weight. Weights are relative (sum doesn't need to be 100; they're normalized). References built-in or custom templates.
- `contention` — optional. Default: `distributed`.
- `account_pool.count` — number of accounts. Keys are injected separately.

### 3.8 `faults`

```yaml
faults:
  dry_run: false

  schedule:
    - name: validator-packet-loss
      at: "12m"
      type: packet_loss
      adapter: netem
      target: validator-group-b
      params:
        loss_percent: 5
        correlation: 25
      duration: "3m"

    - name: feed-disconnect
      at: "22m"
      type: feed_disconnect
      adapter: feed_fault
      edges: [us-west-2]
      params:
        disconnect_duration_sec: 30
```

- `dry_run` — if `true`, faults are logged but not executed.
- `schedule` — list of fault events. See RFC-0008 §4.1 for full spec.
- `at` — offset from run start.
- `duration` — auto-clear after this duration. Alternative: `clear_at` for explicit clear time.
- `edges` — optional edge selector. Default: all edges.

### 3.9 `correctness`

```yaml
correctness:
  checks:
    - type: balance_conservation
      tolerance_wei: "1000000000000"

    - type: receipt_completeness
      min_rate: 0.999

    - type: ordering
      mode: intra_account

    - type: cross_observer
      observers:
        - "https://rpc-1.staging.internal/rpc"
        - "https://rpc-2.staging.internal/rpc"
      sample_rate: 0.01

  thresholds:
    correctness_violations: 0
    max_revert_rate: 0.05
    receipt_completeness: 0.999
```

- `checks` — list of oracle checks to enable. See RFC-0007 §4.1.
- `thresholds` — pass/fail criteria. Violations beyond thresholds → run fails.

### 3.10 `regression`

```yaml
regression:
  baseline: "tag:release/v3.9.4"
  thresholds:
    p99_accept_latency_ms:
      max_regression_pct: 10
    max_sustainable_gas_sec:
      max_regression_pct: 5
    correctness_violations:
      max_value: 0
```

- `baseline` — reference run. `"run_id:<uuid>"` or `"tag:<name>"`.
- `thresholds` — per-metric regression thresholds. See RFC-0009 §4.2.

### 3.11 `templates` (Custom)

```yaml
templates:
  custom-swap:
    type: contract_call
    contract: "0x1234...abcd"
    abi_file: "./abis/router.json"
    function: "swapExactTokensForTokens"
    params:
      amountIn:
        type: random_range
        min: "1000000"
        max: "1000000000"
      amountOutMin: "0"
      path:
        type: fixed
        value: ["0xtoken_a", "0xtoken_b"]
      to:
        type: sender
      deadline:
        type: block_timestamp_plus
        offset: 300
    estimated_gas: 250000
```

Custom templates are available in the `profiles` section by name.

### 3.12 `incident_replay`

```yaml
incident_replay:
  incident_id: "INC-2025-0301"
  source_file: "./incident-data/gas-timeseries.csv"
  scaling_factor: 1.0
  expected_symptoms:
    - name: "p99 latency spike >500ms"
      metric: accept_latency_p99_ms
      condition: gt
      value: 500
    - name: "validator lag >5 blocks"
      metric: validator_lag_blocks
      condition: gt
      value: 5
```

---

## 4. JSON Schema Validation

The SDL is validated against a JSON Schema before compilation. The schema is versioned and bundled with the `gashammer-scenario` crate.

```text
crates/gashammer-scenario/
├── schemas/
│   └── v1.0/
│       ├── scenario.schema.json    # root schema
│       ├── workload.schema.json    # workload sub-schema
│       ├── fault.schema.json       # fault sub-schema
│       ├── correctness.schema.json
│       └── template.schema.json
├── src/
│   ├── compiler.rs
│   ├── validator.rs
│   └── partitioner.rs
```

Validation produces structured error messages:

```json
{
  "errors": [
    {
      "path": "workload.profiles[2].template",
      "message": "Unknown template 'defi-heavy'. Available: simple-transfer, erc20-transfer, compute-heavy, calldata-heavy, noop",
      "severity": "error"
    },
    {
      "path": "edges.region_allocation",
      "message": "Region percentages sum to 95, expected 100",
      "severity": "error"
    },
    {
      "path": "faults.schedule[0].adapter",
      "message": "Adapter 'netem' requires CAP_NET_ADMIN. Verify edge capabilities at preflight.",
      "severity": "warning"
    }
  ]
}
```

---

## 5. Compilation Pipeline

```text
YAML Source
    │
    ▼
┌──────────────┐
│ Parse        │  serde_yaml → ScenarioDefinition struct
└──────┬───────┘
       ▼
┌──────────────┐
│ Validate     │  JSON Schema check + semantic validation
└──────┬───────┘
       ▼
┌──────────────┐
│ Resolve      │  Resolve template references, ABI files, seed
└──────┬───────┘
       ▼
┌──────────────┐
│ Partition    │  Split workload across edges by region
│              │  (RFC-0005 §7)
└──────┬───────┘
       ▼
┌──────────────┐
│ Emit         │  CompiledScenario + per-edge WorkloadSlices
└──────────────┘
```

### 5.1 Semantic Validation (Beyond Schema)

The compiler performs additional checks that JSON Schema cannot express:

- Phase durations sum to total duration
- Region allocation sums to 100
- Referenced templates exist (built-in or custom)
- Custom template ABI files are readable and valid
- Account pool count is sufficient for target tx rate (RFC-0005 §6.2)
- Fault timing is within run duration
- Fault adapters are compatible with declared edge capabilities
- Correctness check configurations are valid (e.g., cross-observer URLs are reachable)
- Chain type matches correctness check compatibility (e.g., replay checks need rollup, not anytrust)

### 5.2 Compilation Output

```rust
pub struct CompiledScenario {
    pub scenario_id: Uuid,
    pub name: String,
    pub seed: u64,
    pub duration: Duration,
    pub phases: Vec<CompiledPhase>,
    pub edge_assignments: Vec<EdgeAssignment>,
    pub fault_timeline: FaultTimeline,
    pub correctness_config: CorrectnessConfig,
    pub regression_config: Option<RegressionConfig>,
    pub target: TargetConfig,
    pub source_hash: String,       // SHA-256 of original YAML
    pub compiler_version: String,
    pub compiled_at: DateTime<Utc>,
}
```

The compiled scenario is serialized to JSON and stored as an immutable artifact (RFC-0003 §4.4).

---

## 6. Example Scenarios

### 6.1 Minimal Scenario

```yaml
version: "1.0"

scenario:
  name: "quick-smoke-test"
  seed: 1

target:
  chain_id: 421614
  chain_type: rollup
  sequencer_rpc: "https://sepolia-rollup-sequencer.arbitrum.io/rpc"
  gateway_rpc: "https://sepolia-rollup.arbitrum.io/rpc"
  feed_relay: "ws://feed-relay.local:9642"
  l1_rpc: "https://sepolia.infura.io/v3/KEY"

duration: "5m"

phases:
  - name: sustain
    duration: "5m"

edges:
  region_allocation:
    us-east-1: 100

workload:
  target_gas_rate:
    mode: sustained
    value: 5000000
  profiles:
    - template: simple-transfer
      weight: 100
  account_pool:
    count: 100
    funding: prefunded

correctness:
  thresholds:
    correctness_violations: 0
```

### 6.2 Full Multi-Region Burst Test (from PRD §10.1)

```yaml
version: "1.0"

scenario:
  name: "sequencer-global-burst"
  description: "Multi-region burst test for sequencer capacity envelope"
  tags: ["sequencer", "burst", "release-gate"]
  seed: 42

target:
  chain_id: 421614
  chain_type: rollup
  sequencer_rpc: "https://sequencer.staging.internal/rpc"
  gateway_rpc: "https://rpc.staging.internal/rpc"
  feed_relay: "ws://feed-relay.staging.internal:9642"
  l1_rpc: "https://l1.staging.internal/rpc"
  prometheus_endpoints:
    - name: sequencer
      url: "http://sequencer.staging.internal:6070/debug/metrics"
    - name: validator
      url: "http://validator.staging.internal:6070/debug/metrics"

duration: "30m"

phases:
  - name: ramp
    duration: "5m"
    workload:
      target_gas_rate:
        mode: ramped
        start: 10000000
        end: 100000000

  - name: sustain
    duration: "20m"

  - name: recover
    duration: "5m"
    workload:
      target_gas_rate:
        mode: sustained
        value: 0

edges:
  region_allocation:
    us-west-2: 30
    us-east-1: 20
    eu-central-1: 25
    ap-southeast-1: 25
  edge_policy: best_effort
  min_edges_per_region: 1

workload:
  target_gas_rate:
    mode: sustained
    value: 100000000
  profiles:
    - template: simple-transfer
      weight: 40
    - template: erc20-transfer
      weight: 25
    - template: compute-heavy
      weight: 20
    - template: calldata-heavy
      weight: 15
  contention:
    mode: distributed
  account_pool:
    count: 50000
    funding: prefunded

faults:
  schedule:
    - name: validator-packet-loss
      at: "12m"
      type: packet_loss
      adapter: netem
      target: validator-group-b
      params:
        loss_percent: 5
      duration: "3m"

correctness:
  checks:
    - type: balance_conservation
    - type: receipt_completeness
      min_rate: 0.999
    - type: ordering
      mode: intra_account
  thresholds:
    correctness_violations: 0
    max_revert_rate: 0.05
    p99_accept_latency_ms: 1500
    max_validator_lag_blocks: 3

regression:
  baseline: "tag:release/v3.9.4"
  thresholds:
    p99_accept_latency_ms:
      max_regression_pct: 10
    max_sustainable_gas_sec:
      max_regression_pct: 5
```

---

## 7. CLI Interface

### 7.1 Scenario Commands

```text
gashammer scenario validate <file.yaml>
    Validate a scenario without running it.
    Checks: schema, semantic, template resolution.
    Does NOT check edge availability or target reachability.

gashammer scenario compile <file.yaml>
    Compile a scenario into per-edge assignments.
    Requires: hive connection (to resolve edge availability).
    Outputs: compiled scenario JSON.

gashammer scenario run <file.yaml>
    Submit, compile, and run a scenario.
    Equivalent to: POST /api/v1/scenarios + POST /api/v1/runs

gashammer scenario list
    List stored scenarios (from hive API).

gashammer run status <run_id>
    Get current run status.

gashammer run report <run_id> [--format json|md|html]
    Fetch the run report.

gashammer run abort <run_id>
    Abort a running scenario.
```

### 7.2 CLI Design (following hashimoto-cli-ux principles)

- Consistent subcommand pattern: `gashammer <noun> <verb>`
- `--format` flag for output format (default: human-readable table)
- `--json` shorthand for machine-readable output
- Non-zero exit code on failure
- Colored output when attached to TTY; plain when piped
- `--quiet` flag suppresses progress; only outputs final result

---

## 8. Risks

### Risk: YAML indentation errors cause silent misconfiguration

YAML is sensitive to indentation; a misplaced field may be silently ignored.

**Mitigation:** Strict JSON Schema validation rejects unknown fields (`additionalProperties: false`). The compiler warns on any YAML key not in the schema.

### Risk: SDL grows too complex for simple use cases

As features accumulate, the SDL may become intimidating.

**Mitigation:** Progressive complexity — most fields are optional with sensible defaults. The minimal scenario (§6.1) is 30 lines. Provide a `gashammer scenario init` command that generates a starter template.

### Risk: ABI file references break portability

Relative path references to ABI files may not resolve correctly in different environments.

**Mitigation:** ABI files are resolved relative to the scenario YAML file's directory. The compiled scenario embeds resolved ABIs (not file paths), making the compiled artifact self-contained.

### Risk: Schema versioning creates compatibility headaches

Multiple SDL versions in the wild may confuse users and the compiler.

**Mitigation:** The compiler supports N and N-1 schema versions. Clear migration guides between versions. The `version` field is required and validated first.

---

## 9. Open Questions

1. Should the SDL support includes/imports (e.g., `!include workload-profiles/defi.yaml`)?
2. Should there be a scenario library of pre-built scenarios for common test patterns?
3. Should the SDL support parameterization (e.g., `${TARGET_GAS_RATE}` resolved from environment)?
4. Should `gashammer scenario init` be interactive or generate a static template?
5. Should the SDL support conditional logic (e.g., "if Timeboost is enabled, add express lane workload")?

---

## 10. References

- PRD §6.4 (Scenarios are code)
- PRD §8.2 (Scenario concept)
- PRD §10.1 (Scenario Definition Language — requirements + example)
- RFC-0003 §4 (Scenario Lifecycle — compilation, validation)
- RFC-0005 §2–4 (Templates, gas profiles, temporal patterns)
- RFC-0005 §5 (Contention modes)
- RFC-0005 §7 (Workload partitioning)
- RFC-0007 §4 (Invariant framework — scenario configuration)
- RFC-0008 §4 (Fault scheduling — scenario configuration)
- RFC-0009 §4 (Regression — baseline configuration)
