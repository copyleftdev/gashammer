# Scenario Best Practices

## Start Small, Ramp Up

Begin with a low gas rate (1 Mgas/s) and short duration. Validate that the pipeline works before increasing load. The `quick-smoke-test.yaml` scenario is designed for this.

## Use Deterministic Seeds

Always set the `seed` field in production scenarios. This ensures reproducibility: the same seed with the same configuration produces the same transaction sequence. When comparing results across runs, identical seeds eliminate workload variance as a variable.

```yaml
workload:
  seed: 42
```

## Include Warmup and Cooldown Phases

Sudden load changes can produce misleading metrics. Include at least a 30-second warmup phase at reduced rate and a 30-second cooldown:

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

## Mix Templates for Realism

Pure transfer workloads under-represent real usage. Include storage writes, contract calls, and compute-heavy transactions to exercise the full execution pipeline. Weight them according to your expected traffic profile.

## Use Ramp Scenarios to Find Limits

The `ramp-to-saturation` pattern progressively increases load until the system degrades. This is the most reliable way to determine the safe operating envelope. Run it before committing to a target gas rate for sustained tests.

## Enable Oracle Checks for Correctness

For correctness-focused testing, enable all oracle invariants and keep the gas rate low enough that the oracle can verify every transaction:

```yaml
oracle:
  invariants:
    - balance-conservation
    - nonce-monotonicity
    - gas-accounting
  check_interval_secs: 5
```

## Inject Faults Sequentially

When testing fault tolerance, inject one fault at a time and clear it before the next. This makes it easier to correlate observed behavior with specific faults. Always leave recovery time between faults.

## Name Phases Descriptively

Phase names appear in reports. Use names that describe the intent (`warmup`, `steady-state-5mgas`, `fault-latency-200ms`) rather than generic labels (`phase1`, `phase2`).

## Version Your Scenarios

Include a `version` field and update it when the scenario changes. This makes it possible to correlate reports with specific scenario definitions.
