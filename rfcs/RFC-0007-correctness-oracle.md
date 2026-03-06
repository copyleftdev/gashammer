# RFC-0007: Correctness Oracle

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0001 (Nitro Integration Surface)
- RFC-0004 (Edge Runtime)
- RFC-0006 (Telemetry & Metrics Pipeline)

## Summary

This RFC defines the **Correctness Oracle** — the subsystem that verifies the system under test produced correct results under load. GasHammer's core thesis is that **a faster but inconsistent system is a failed result** (PRD §6.2). The oracle validates transaction outcomes, state consistency, ordering invariants, and replay equivalence during and after a run.

---

## 1. Design Principles

1. **Correctness is binary at the assertion level** — each check passes or fails; no "soft" correctness
2. **Violations are evidence, not opinions** — every failure includes the concrete data that proves the violation
3. **Oracle must not distort the test** — correctness checks run with lower priority than workload generation
4. **Checks are composable** — scenarios select which oracle checks to enable
5. **Post-run checks are as important as live checks** — some invariants can only be verified after the run completes

---

## 2. Oracle Architecture

```text
┌────────────────────────────────────────────────────┐
│                gashammer-oracle                     │
│                                                     │
│  ┌──────────────┐  ┌──────────────┐  ┌───────────┐ │
│  │ Live Checks  │  │ Post-Run     │  │ Invariant │ │
│  │ (during run) │  │ Checks       │  │ Registry  │ │
│  └──────┬───────┘  └──────┬───────┘  └─────┬─────┘ │
│         │                 │                 │       │
│  ┌──────┴─────────────────┴─────────────────┴────┐  │
│  │              Verdict Engine                   │  │
│  │  pass/fail per check → aggregate verdict      │  │
│  └───────────────────┬───────────────────────────┘  │
│                      │                              │
│  ┌───────────────────┴───────────────────────────┐  │
│  │              Evidence Store                   │  │
│  │  violation details → artifacts                │  │
│  └───────────────────────────────────────────────┘  │
└────────────────────────────────────────────────────┘
```

---

## 3. Check Categories

### 3.1 Transaction Outcome Checks (Live)

Run during the test as receipts arrive.

| Check | Description | Failure Condition |
| --- | --- | --- |
| **Receipt status** | Every non-adversarial tx should succeed | Unexpected revert (outside declared contention mode) |
| **Gas used bounds** | Gas used should be within expected range of template estimate | Gas used deviates >50% from estimate without explanation |
| **Log emission** | Txs that should emit events must have matching logs | Missing expected log topics |
| **Receipt completeness** | Every submitted tx must eventually have a receipt or explicit timeout | Tx disappears (no receipt, no revert, no timeout) |

### 3.2 State Consistency Checks (Post-Run)

Run after the test completes, querying final chain state.

| Check | Description | Failure Condition |
| --- | --- | --- |
| **Balance conservation** | Sum of account balances + gas spent = initial funding | Discrepancy beyond rounding tolerance |
| **Storage integrity** | Known storage slots contain expected values after known writes | Slot value doesn't match expected state |
| **Contract state** | Contract-specific invariants (e.g., ERC-20 totalSupply = sum of balances) | Invariant violation |
| **Nonce consistency** | Final on-chain nonce for each account = initial nonce + successful tx count | Mismatch indicates dropped or phantom txs |

### 3.3 Ordering Checks (Post-Run)

Verify that transaction ordering matches expected behavior.

| Check | Description | Failure Condition |
| --- | --- | --- |
| **Sequence monotonicity** | Sequencer feed sequence numbers are strictly increasing per edge | Gap or reversal |
| **Intra-account ordering** | Txs from the same account are included in nonce order | Out-of-order nonce inclusion |
| **Timestamp ordering** | Block timestamps are non-decreasing | Timestamp regression |

### 3.4 Cross-Observer Checks (Post-Run)

Compare observations from multiple edges or nodes.

| Check | Description | Failure Condition |
| --- | --- | --- |
| **Receipt equivalence** | Multiple edges querying the same tx hash get identical receipts | Receipt fields differ |
| **Log equivalence** | Same block queried from different RPC endpoints returns identical logs | Log divergence |
| **Block equivalence** | Same block number returns identical block hash from different observers | Block hash mismatch (fork detected) |

### 3.5 Replay Checks (Post-Run, Optional)

Compare execution against a reference replay.

| Check | Description | Failure Condition |
| --- | --- | --- |
| **State root match** | If two nodes replay the same blocks, they should produce the same state root | State root divergence |
| **Receipt root match** | Receipt trie roots should match across replay | Receipt root divergence |

Replay checks require access to multiple nodes or a reference execution trace. They are expensive and optional.

---

## 4. Invariant Framework

### 4.1 Invariant Definition

Users can define custom invariants in scenario YAML:

```yaml
correctness:
  checks:
    - type: balance_conservation
      tolerance_wei: "1000000000000"  # 1000 gwei tolerance for rounding

    - type: storage_integrity
      contract: "0x1234...abcd"
      slots:
        - key: "0x0"
          expected: "nonzero"

    - type: custom_invariant
      name: "erc20-supply-check"
      contract: "0xTOKEN"
      abi_file: "./abis/erc20.json"
      check:
        function: "totalSupply"
        condition: "eq"
        reference_value: "1000000000000000000000000"

    - type: ordering
      mode: intra_account

    - type: cross_observer
      observers:
        - "https://rpc-1.staging.internal/rpc"
        - "https://rpc-2.staging.internal/rpc"
      sample_rate: 0.01  # check 1% of txs
```

### 4.2 Invariant Trait

```rust
pub trait Invariant: Send + Sync {
    fn name(&self) -> &str;

    fn check_type(&self) -> CheckType; // Live or PostRun

    /// For live checks: called per event
    fn check_event(&self, event: &TelemetryEvent, ctx: &OracleContext)
        -> Option<Verdict>;

    /// For post-run checks: called once after run completes
    async fn check_final(&self, ctx: &OracleContext)
        -> Verdict;
}

pub enum CheckType {
    Live,
    PostRun,
    Both,
}

pub struct Verdict {
    pub check_name: String,
    pub passed: bool,
    pub evidence: Option<Evidence>,
}

pub struct Evidence {
    pub description: String,
    pub expected: String,
    pub actual: String,
    pub tx_hash: Option<TxHash>,
    pub block_number: Option<u64>,
    pub raw_data: Option<serde_json::Value>,
}
```

### 4.3 Built-in Invariants

| Invariant | `CheckType` | Enabled by Default |
| --- | --- | --- |
| `ReceiptStatusCheck` | Live | Yes |
| `GasUsedBoundsCheck` | Live | Yes |
| `ReceiptCompletenessCheck` | Live | Yes |
| `BalanceConservationCheck` | PostRun | Yes |
| `NonceConsistencyCheck` | PostRun | Yes |
| `SequenceMonotonicityCheck` | Live | Yes |
| `IntraAccountOrderingCheck` | PostRun | No (requires tx tracing) |
| `CrossObserverReceiptCheck` | PostRun | No (requires multiple RPCs) |
| `StorageIntegrityCheck` | PostRun | No (requires config) |
| `CustomContractInvariant` | PostRun | No (requires config) |

---

## 5. Oracle Execution Model

### 5.1 Live Checks

Live checks run in the hive as events arrive from the telemetry pipeline:

1. Telemetry events flow through the ingestion router (RFC-0006 §4.2)
2. A parallel branch routes events to the oracle's live check engine
3. Each live invariant receives events via a bounded channel
4. Verdicts are emitted as `CorrectnessEvent` telemetry events themselves
5. Violations increment a `correctness_violations` counter (PRD §10.1 threshold)

**Performance constraint:** Live checks must process events faster than ingestion rate. If they fall behind, events are sampled (not dropped from the main pipeline).

### 5.2 Post-Run Checks

Post-run checks execute after the run transitions to `Completing` state (RFC-0003 §5.1):

1. Hive signals oracle to begin post-run checks
2. Oracle queries chain state via RPC gateway (using hive's connection, not edge connections)
3. Each post-run invariant executes independently and concurrently
4. All verdicts collected → aggregate pass/fail
5. Results stored in run metadata and included in report

### 5.3 Sampling for Expensive Checks

Cross-observer and replay checks are expensive. They support a `sample_rate` parameter:

- `1.0` = check every tx (expensive)
- `0.01` = check 1% of txs (default for cross-observer)
- `0.0` = disabled

Sampled checks select txs deterministically (hash-based) so the same txs are checked on re-run.

---

## 6. Verdict Aggregation

### 6.1 Per-Check Verdict

Each invariant produces a final verdict:

```rust
pub struct CheckSummary {
    pub check_name: String,
    pub total_checked: u64,
    pub passed: u64,
    pub failed: u64,
    pub skipped: u64,
    pub violations: Vec<Evidence>,  // capped at 100 per check
    pub verdict: bool,              // true if failed == 0
}
```

### 6.2 Run-Level Verdict

```rust
pub struct CorrectnessVerdict {
    pub run_id: Uuid,
    pub overall_pass: bool,
    pub checks: Vec<CheckSummary>,
    pub total_violations: u64,
    pub threshold_violations: Vec<ThresholdViolation>,
}
```

`overall_pass` is `true` only if:

- All enabled checks have zero failures
- `correctness_violations` count is within the scenario threshold (typically 0)

### 6.3 Threshold Integration

The scenario's `thresholds` section (PRD §10.1) includes correctness thresholds:

```yaml
thresholds:
  correctness_violations: 0
  max_revert_rate: 0.05         # 5% revert rate acceptable for adversarial workloads
  receipt_completeness: 0.999   # 99.9% of txs must have receipts
```

---

## 7. Evidence Retention

Every violation produces an `Evidence` record that includes enough data to reproduce and diagnose the issue:

- **Tx hash** and block number
- **Expected vs actual** values
- **Raw RPC response** (JSON) for the violating query
- **Timestamp** of the check
- **Edge and region** that observed the violation (for cross-observer checks)

Evidence is stored as part of the run's artifact bundle in S3 (RFC-0003 §4.4).

---

## 8. Risks

### Risk: Oracle RPC queries contend with edge workload queries

Post-run oracle checks query the same RPC gateway as edges.

**Mitigation:** Post-run checks execute after workload generation stops (during `Completing` phase). They use the hive's own RPC connection, not edge connections. Rate-limit oracle queries to avoid overwhelming the target.

### Risk: State changes between tx inclusion and oracle state check

Chain state evolves between when a tx was included and when the oracle queries state.

**Mitigation:** Oracle reads state at a specific block number (the last block of the run), not "latest." Use `eth_call` with block number parameter.

### Risk: Cross-observer checks fail due to node sync lag, not correctness

Different RPC nodes may be at different block heights.

**Mitigation:** Wait for all observer nodes to reach the same block height before cross-observer checks. Timeout and skip with warning if sync doesn't converge within 60s.

### Risk: Custom invariants have bugs that produce false violations

User-defined invariants may be incorrectly specified.

**Mitigation:** Oracle logs full evidence for every violation. The report engine presents violations with raw data so engineers can verify. Custom invariants are clearly labeled as user-defined in the report.

---

## 9. Open Questions

1. Should the oracle support "warning" verdicts (non-blocking violations) in addition to pass/fail?
2. Should live checks be able to trigger an early abort if violation count exceeds a threshold?
3. How should the oracle handle txs that are included but produce different logs on different query attempts (transient RPC inconsistency vs. real issue)?
4. Should the oracle integrate with formal verification tools for Stylus contracts?
5. What is the minimum set of checks for a compelling MVP demo?

---

## 10. References

- PRD §6.2 (Correctness matters as much as throughput)
- PRD §10.5 (Correctness Verification)
- PRD §14 (Reporting — Correctness Summary)
- RFC-0001 §3 (Transaction Submission — receipt data)
- RFC-0001 §5 (RPC Gateway — state queries)
- RFC-0001 §7 (Validator / BoLD)
- RFC-0004 §7 (Receipt Tracker — TxReceiptRecord)
- RFC-0006 §2 (Event Model)
