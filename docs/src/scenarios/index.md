# Scenario Authoring

GasHammer scenarios are defined in YAML using the **Scenario Definition Language (SDL)**. A scenario describes the workload to generate, the phases to execute, and the correctness checks to apply.

## Core Concepts

- **Scenario** --- a complete test definition including metadata, phases, workload configuration, and oracle settings
- **Phase** --- a time segment within a run, each with its own gas rate and duration
- **Template** --- a transaction type (e.g., simple transfer, ERC-20 transfer, storage write) with a known gas profile
- **Gas Rate** --- workloads are defined by target gas per second, not transactions per second
- **Fault Schedule** --- optional controlled failures injected during the run

## Scenario Structure

```yaml
name: my-scenario
description: "What this scenario tests"
version: "1.0"

target:
  chain_id: 412346
  sequencer_rpc: "http://sequencer:8547"

workload:
  gas_rate:
    type: sustained
    gas_per_sec: 5_000_000
  templates:
    - name: simple-transfer
      weight: 70
    - name: erc20-transfer
      weight: 30

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

## Sections

- [SDL Reference](./sdl-reference.md) --- complete field-by-field reference for all SDL options
- [Examples](./examples.md) --- walkthrough of example scenarios
- [Best Practices](./best-practices.md) --- tips for writing effective scenarios
