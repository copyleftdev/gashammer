# RFC-0005: Workload Engine & Gas Modeling

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0002 (Technology Stack & Dependencies)
- RFC-0004 (Edge Runtime)

## Summary

This RFC defines how GasHammer models, generates, and controls workloads. The workload engine is the core of what makes GasHammer a **gas-first** load testing platform rather than a generic HTTP load tester. It specifies transaction templates, gas profile modeling, temporal patterns (burst/steady/adversarial), contention modes, and the rate control mechanisms that govern how load is shaped and applied.

---

## 1. Design Principles

1. **Gas is the primary currency** — workloads are defined and measured in gas, not requests per second
2. **Templates are composable** — transaction types are building blocks; workload profiles mix them
3. **Determinism is mandatory** — same seed produces same tx sequence (PRD §11 Determinism)
4. **Contention is explicit** — adversarial access patterns are a first-class concept, not an accident
5. **Rate control is precise** — gas/sec governors must be accurate within 5% over any 10-second window

---

## 2. Transaction Templates

### 2.1 Concept

A transaction template is a reusable definition of a transaction type. It specifies what contract to call (or whether it's a simple transfer), what ABI to use, how to parameterize the call, and the expected gas cost.

### 2.2 Template Structure

```rust
pub struct TxTemplate {
    pub name: String,
    pub description: String,
    pub tx_type: TxType,
    pub estimated_gas: u64,
    pub gas_variance: f64,           // 0.0–1.0; how much gas can vary per invocation
    pub parameterizer: Box<dyn Parameterizer>,
}

pub enum TxType {
    /// Simple ETH transfer
    Transfer {
        value_wei: U256,
    },
    /// Contract call with ABI
    ContractCall {
        contract_address: Address,
        abi: JsonAbi,
        function_name: String,
    },
    /// Raw calldata (pre-encoded)
    RawCalldata {
        to: Address,
        data: Bytes,
    },
    /// ArbOS precompile interaction (RFC-0001 §8)
    Precompile {
        precompile: PrecompileTarget,
    },
}

pub enum PrecompileTarget {
    ArbSys,           // 0x64 — L2→L1 messaging, block queries
    ArbGasInfo,       // 0x6C — gas price queries
    ArbRetryableTx,   // 0x6E — retryable ticket operations
    NodeInterface,    // 0xC8 — gas estimation helpers
}
```

### 2.3 Parameterizer Trait

Each template has a parameterizer that generates unique calldata per invocation:

```rust
pub trait Parameterizer: Send + Sync {
    /// Generate calldata for the next invocation
    fn next_calldata(&self, rng: &mut ChaCha20Rng, account: &Account) -> Bytes;

    /// Estimate gas for this parameterization (can use cached value)
    fn estimated_gas(&self) -> u64;
}
```

Examples:

- **Transfer parameterizer:** Randomly selects a recipient from the account pool; value is fixed or randomized within a range
- **ERC-20 parameterizer:** Encodes `transfer(to, amount)` with randomized `to` and `amount`
- **DeFi parameterizer:** Encodes a swap call with randomized token amounts
- **Pathological parameterizer:** Generates calldata designed to maximize state reads/writes or trigger worst-case EVM execution paths

### 2.4 Built-in Templates

GasHammer ships with these templates for MVP:

| Template | Gas Estimate | Description |
| --- | --- | --- |
| `simple-transfer` | 21,000 | ETH transfer between accounts |
| `erc20-transfer` | ~65,000 | ERC-20 token transfer |
| `erc20-approve` | ~46,000 | ERC-20 approve |
| `storage-write` | ~43,000 | Write a single storage slot (minimal contract) |
| `storage-write-heavy` | ~200,000 | Write 10 storage slots |
| `event-emit` | ~30,000 | Emit a single event |
| `event-emit-heavy` | ~150,000 | Emit 10 events with indexed params |
| `compute-heavy` | ~500,000 | Loop-based CPU-intensive computation |
| `calldata-heavy` | ~100,000 | Large calldata (tests L1 posting cost) |
| `precompile-arbsys` | ~50,000 | ArbSys.arbBlockNumber() |
| `precompile-gasinf` | ~50,000 | ArbGasInfo.getL1BaseFeeEstimate() |
| `noop` | 21,000 | Self-transfer (baseline) |

### 2.5 Custom Templates

Users define custom templates in scenario YAML by specifying:

- Contract address
- ABI (inline or file reference)
- Function name
- Parameter generation rules (fixed, random range, pool selection)

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

---

## 3. Gas Profile Modeling

### 3.1 Why Gas Profiles

A "10,000 tx/sec" workload can mean wildly different things depending on the gas distribution. 10,000 simple transfers is ~210M gas/sec. 10,000 DeFi swaps at 250K gas each is ~2.5B gas/sec. GasHammer models workload by gas shape, not by tx count.

### 3.2 Gas Distribution Model

A workload profile specifies a mix of templates with weights:

```rust
pub struct WorkloadProfile {
    pub name: String,
    pub templates: Vec<WeightedTemplate>,
    pub target_gas_rate: GasRate,
    pub contention_mode: ContentionMode,
}

pub struct WeightedTemplate {
    pub template_name: String,
    pub weight: f64,          // 0.0–1.0; sum of all weights must = 1.0
}

pub enum GasRate {
    Sustained(u64),           // constant gas/sec
    Ramped {
        start: u64,
        end: u64,
        duration: Duration,
    },
    Bursty {
        baseline: u64,
        burst_peak: u64,
        burst_duration: Duration,
        burst_interval: Duration,
    },
    Custom(Vec<(Duration, u64)>), // piecewise-linear gas/sec schedule
}
```

### 3.3 Effective Gas Calculation

For a workload profile, the **expected gas per transaction** is:

```
E[gas/tx] = Σ (weight_i × estimated_gas_i)
```

The **target tx/sec** to achieve a gas/sec target is:

```
target_tx_sec = target_gas_sec / E[gas/tx]
```

The rate limiter (RFC-0004 §5.2) uses this to convert gas/sec targets into token refill rates.

### 3.4 Gas Variance

Templates have a `gas_variance` parameter (0.0–1.0) that models real-world variability:

- `0.0` = fixed gas (e.g., simple transfer is always 21,000)
- `0.3` = moderate variance (e.g., ERC-20 transfer varies by token implementation)
- `0.8` = high variance (e.g., DeFi swap depends on pool state)

The rate limiter accounts for variance by using `estimated_gas × (1 + variance/2)` as a conservative token cost. This prevents systematic under- or over-shooting the gas target.

---

## 4. Temporal Patterns

### 4.1 Steady State

Constant gas/sec for the duration of a phase. Used for sustained throughput measurement.

```yaml
workload:
  gas_rate: 50000000  # 50M gas/sec
  mode: sustained
```

### 4.2 Ramp

Linear increase from a start rate to an end rate over a phase duration. Used for finding saturation points.

```yaml
workload:
  gas_rate:
    mode: ramped
    start: 10000000   # 10M gas/sec
    end: 200000000    # 200M gas/sec
    duration: 10m
```

### 4.3 Burst

Periodic spikes over a baseline. Used for testing backlog recovery and queue behavior.

```yaml
workload:
  gas_rate:
    mode: bursty
    baseline: 30000000     # 30M gas/sec baseline
    burst_peak: 150000000  # 150M gas/sec during bursts
    burst_duration: 10s
    burst_interval: 60s
```

### 4.4 Custom

Piecewise-linear schedule for modeling specific production traffic patterns or incident replays.

```yaml
workload:
  gas_rate:
    mode: custom
    schedule:
      - [0s, 10000000]
      - [30s, 50000000]
      - [60s, 200000000]  # spike
      - [70s, 200000000]  # hold spike
      - [80s, 20000000]   # drop
      - [300s, 20000000]  # steady
```

---

## 5. Contention Modes

### 5.1 Why Contention Matters

Real blockchain workloads exhibit contention patterns that stress different parts of the system:

- **Hot account contention:** Many txs targeting the same contract (e.g., DEX router)
- **State slot contention:** Multiple txs reading/writing the same storage slots
- **Nonce contention:** Multiple txs from the same account competing for nonce ordering
- **Spam patterns:** High volumes of low-value txs designed to saturate mempool

### 5.2 Contention Model

```rust
pub enum ContentionMode {
    /// No intentional contention — accounts and targets are spread evenly
    Distributed,

    /// Hot-account contention — all txs target a single contract address
    HotAccount {
        target: Address,
        contention_ratio: f64,  // 0.0–1.0; fraction of txs targeting the hot account
    },

    /// State-slot contention — txs read/write overlapping storage slots
    HotSlot {
        contract: Address,
        slot_count: u32,        // number of contested slots
        overlap_ratio: f64,     // probability of slot collision per tx pair
    },

    /// Nonce race — multiple txs submitted for the same nonce from the same account
    NonceRace {
        accounts: Vec<Address>,
        duplicates_per_nonce: u32,
    },

    /// Spam flood — maximum tx count at minimum gas
    SpamFlood {
        tx_template: String,    // typically "noop" or "simple-transfer"
    },
}
```

### 5.3 Contention Configuration in Scenarios

```yaml
workload:
  contention:
    mode: hot_account
    target: "0xDEX_ROUTER_ADDRESS"
    contention_ratio: 0.6  # 60% of txs hit the DEX router
```

### 5.4 Contention Metrics

GasHammer measures the **effect** of contention:

| Metric | Description |
| --- | --- |
| `contention_revert_rate` | Fraction of txs that revert (possibly due to state contention) |
| `contention_gas_waste` | Gas consumed by reverted txs |
| `contention_nonce_errors` | Nonce-related submission failures |
| `contention_latency_delta_ms` | Latency difference between contested vs. uncontested txs |

---

## 6. Account Pool Management

### 6.1 Pool Structure

Each scenario defines an account pool that is partitioned across edges (RFC-0003 §4.3, RFC-0004 §5.3):

```rust
pub struct ScenarioAccountPool {
    pub accounts: Vec<AccountConfig>,
    pub funding_source: FundingSource,
    pub total_gas_budget: U256,
}

pub struct AccountConfig {
    pub private_key: SigningKey,  // injected via secrets, not in YAML
    pub initial_balance: U256,
    pub assigned_edge: Option<Uuid>,
}

pub enum FundingSource {
    PreFunded,                    // accounts already funded (test environment)
    FundFromFaucet { url: String },
    FundFromWhale { key: SigningKey, amount_per_account: U256 },
}
```

### 6.2 Account Count Sizing

Rule of thumb for sizing account pools:

```
min_accounts = max_tx_per_sec × avg_receipt_wait_sec
```

At 10,000 tx/sec with 2s average receipt wait, you need ≥20,000 accounts to avoid nonce starvation (where all accounts have pending unconfirmed txs).

The scenario compiler validates this and warns if the pool is undersized.

### 6.3 Key Injection

Private keys are **never** stored in scenario YAML (PRD §11 Security). They are injected via:

- Environment variable: `GASHAMMER_ACCOUNT_KEYS=<comma-separated hex keys>`
- Mounted secret file: `/etc/gashammer/secrets/accounts.json`
- K8s Secret volume
- For test environments: deterministic key derivation from a master key + index (reproducible but not production-safe)

---

## 7. Workload Partitioning

### 7.1 Problem

A scenario defines a global workload (e.g., 100M gas/sec, 40% transfers, 25% ERC-20, 20% DeFi, 15% pathological). This must be split across N edges in M regions while preserving the global distribution and determinism.

### 7.2 Algorithm

1. **Regional split:** Allocate gas budget per region based on scenario percentages
2. **Per-edge split:** Within a region, divide equally across healthy edges
3. **Template distribution:** Each edge gets the same template weight distribution (no per-edge template skew)
4. **Account assignment:** Disjoint account pools per edge
5. **Seed derivation:** `edge_seed = master_seed XOR (region_index << 32 | edge_index)`

```rust
pub fn partition_workload(
    scenario: &CompiledScenario,
    edges: &[EdgeRecord],
) -> Vec<EdgeAssignment> {
    let mut assignments = Vec::new();

    for (region, pct) in &scenario.regional_allocation {
        let region_gas_rate = scenario.target_gas_rate * pct;
        let region_edges: Vec<_> = edges.iter()
            .filter(|e| &e.region == region && e.status == EdgeStatus::Healthy)
            .collect();

        let per_edge_gas = region_gas_rate / region_edges.len() as u64;

        for (i, edge) in region_edges.iter().enumerate() {
            let seed = scenario.seed ^ ((region_index as u64) << 32 | i as u64);
            assignments.push(EdgeAssignment {
                edge_id: edge.edge_id,
                region: region.clone(),
                workload_slice: WorkloadSlice {
                    tx_templates: scenario.templates.clone(),
                    gas_rate_limit: GasRate::Sustained(per_edge_gas),
                    account_pool: allocate_accounts(edge, &scenario.account_pool),
                    seed,
                    burst_config: scenario.burst_config.clone(),
                },
                // ...
            });
        }
    }

    assignments
}
```

### 7.3 Validation

The scenario compiler validates:

- Sum of regional percentages = 100%
- Each region has ≥1 healthy edge
- Account pool is large enough for the target tx rate
- Gas budget is fundable from account balances
- Template contracts are deployed on the target chain (via `eth_getCode` check)

---

## 8. Workload Replay

### 8.1 Purpose

For incident replay (PRD User Story: incident responder), GasHammer must support replaying a specific workload pattern derived from production telemetry.

### 8.2 Replay Input Format

```yaml
workload:
  mode: replay
  source: "./incident-2025-03-01.csv"
  format: gas_timeseries
  time_column: timestamp_ms
  gas_column: gas_used
  scaling_factor: 1.0  # 1.0 = exact replay, 2.0 = 2× intensity
```

The replay engine reads the timeseries and generates a `Custom` gas rate schedule that approximates the observed pattern. Template distribution is either inferred from the replay data (if tx types are tagged) or uses a default mix.

### 8.3 Limitations

Replay is approximate, not exact:

- Original account distribution is unknown
- Contract state may differ from production
- Network conditions are not reproduced
- Contention patterns are statistical approximations

The run report notes these limitations in confidence scoring.

---

## 9. Arbitrum-Specific Workload Considerations

### 9.1 L1 Data Cost Awareness

Transactions with large calldata incur higher L1 posting costs. The workload engine tracks:

- Per-template estimated calldata size
- L1 data cost contribution per tx (via `ArbGasInfo` precompile)
- The ratio of L2 computation gas to L1 data gas in the workload mix

This matters because a workload heavy on large calldata will trigger different batch poster behavior (more frequent posting, higher L1 costs) than one heavy on computation.

### 9.2 EIP-1559 Fee Escalation

Under sustained load above the gas target, Arbitrum's EIP-1559 mechanism escalates base fees. The workload engine must:

- Monitor `baseFee` progression during the run
- Adjust `maxFeePerGas` in generated transactions to stay above the rising base fee
- Log fee escalation events as a measurement signal (the system is being pushed)

### 9.3 250ms Block Time

With 250ms blocks, the workload engine operates at a higher temporal resolution than L1 tools:

- Rate limiter tick interval: 50ms (5 ticks per block)
- Burst granularity: minimum 250ms (1 block)
- Receipt polling aligned to 250ms intervals

---

## 10. Risks

### Risk: Gas estimation cache goes stale

Cached gas estimates may diverge from actual costs as chain state evolves during the run.

**Mitigation:** Re-estimate periodically (every 60s) for a sample of templates. If deviation exceeds 20%, emit a warning event. Adjust rate limiter tokens accordingly.

### Risk: Template contract not deployed on target chain

A scenario referencing a custom contract that doesn't exist on the target chain will produce all-revert txs.

**Mitigation:** Preflight checks `eth_getCode` for all template contract addresses. Abort if any return empty.

### Risk: Deterministic replay diverges across runs

Even with fixed seeds, external state (gas price, account balances, chain state) differs between runs, causing execution paths to diverge.

**Mitigation:** Document tolerance bands. Determinism means the *input sequence* is identical; *output behavior* naturally varies. Reports include variance analysis.

### Risk: Contention modes cause cascading reverts

Intentionally adversarial workloads may trigger 90%+ revert rates, wasting gas.

**Mitigation:** This is expected behavior for adversarial testing. The report distinguishes intended contention reverts from unexpected failures. Scenario definitions must declare expected revert rate ranges.

---

## 11. Open Questions

1. Should GasHammer support Stylus (WASM) contract templates in MVP, or defer to post-MVP?
2. How should the workload engine handle dynamic gas prices — should it cap spending or follow market prices?
3. Should replay mode support replaying individual transaction traces (exact calldata) or only aggregate gas patterns?
4. What is the minimum template set needed for a compelling interview demo?
5. Should contention modes be composable (e.g., hot-account + burst simultaneously)?

---

## 12. References

- PRD §6 (Product Principles — Gas-first)
- PRD §8.1 (Gas-Weighted Workload)
- PRD §10.3 (Workload Generation)
- PRD §11 (Determinism NFR)
- RFC-0001 §3 (Transaction Submission — construction requirements)
- RFC-0001 §8 (ArbOS Precompiles)
- RFC-0001 §9 (Gas and Fee Model)
- RFC-0001 §13 (Block Timing Model)
- RFC-0004 §5 (Transaction Generation Pipeline)
- RFC-0004 §5.2 (Rate Limiting)
- RFC-0004 §5.4 (Deterministic Generation)
