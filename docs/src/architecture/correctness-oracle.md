# Correctness Oracle

The oracle verifies that the system under test behaves correctly during and after each run. It evaluates invariants and produces verdicts backed by evidence.

Ref: RFC-0007.

## Invariant Framework

Every correctness check implements the `Invariant` trait:

```rust
trait Invariant: Send + Sync {
    fn check_type(&self) -> CheckType;
    fn name(&self) -> &'static str;
    async fn check(&self, observations: &[Observation]) -> Vec<Verdict>;
}
```

### Check Types

| Type | When | Use Case |
|------|------|----------|
| `Live` | During the run, at `check_interval_secs` | Detect issues early |
| `PostRun` | After the run completes | Full-dataset analysis |
| `Both` | Both times | Double coverage |

### Built-in Invariants

| Invariant | Description |
|-----------|-------------|
| `balance-conservation` | Sum of all account balance changes equals sum of gas fees paid. No ETH created or destroyed. |
| `nonce-monotonicity` | Each account's nonce increases by exactly 1 per included transaction. No gaps, no duplicates. |
| `gas-accounting` | `gas_used` never exceeds `gas_limit`. Reported gas fee matches `gas_used * effective_gas_price`. |

## Verdicts and Evidence

Each check produces a `Verdict`:

```rust
struct Verdict {
    check_name: String,
    passed: bool,
    evidence: Vec<Evidence>,
}

struct Evidence {
    description: String,
    expected: String,
    actual: String,
    tx_hash: Option<String>,
    block_number: Option<u64>,
    raw_data: Option<String>,
}
```

A passing verdict has `passed: true` and empty evidence. A failing verdict includes one or more `Evidence` entries that identify the exact transaction, block, expected value, and actual value.

## Post-Run Aggregation

After all checks complete, the oracle produces a `CorrectnessVerdict`:

```rust
struct CorrectnessVerdict {
    overall_pass: bool,
    summaries: Vec<CheckSummary>,
}

struct CheckSummary {
    check_name: String,
    total_checked: u64,
    passed: u64,
    failed: u64,
    skipped: u64,
    violations: Vec<Evidence>,
}
```

`MAX_VIOLATIONS_PER_CHECK` (100) caps evidence collection to prevent unbounded memory use when many transactions fail the same check.

`overall_pass` is `true` only if every check has zero failures.

## Observations

The oracle does not query the chain directly. It receives `Observation` structs distilled from telemetry events:

```rust
struct Observation {
    tx_hash: String,
    from: String,
    to: String,
    value: u64,
    gas_limit: u64,
    gas_used: Option<u64>,
    effective_gas_price: Option<u64>,
    nonce: u64,
    block_number: Option<u64>,
    status: Option<bool>,
}
```

This decouples the oracle from the transport layer and makes checks testable without a live node.

## Extending the Oracle

To add a custom invariant:

1. Implement the `Invariant` trait.
2. Register it with the oracle during run setup.
3. Reference it by name in the scenario's `oracle.invariants` list.

Testing convention: every invariant must have both a passing and a failing test case, per CLAUDE.md testing rule #8.
