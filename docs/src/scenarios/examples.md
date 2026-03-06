# Scenario Examples

GasHammer ships with several example scenarios in the `scenarios/` directory. This page walks through each one.

## Simple Transfer Load

**File:** `scenarios/simple-transfer-load.yaml`

A basic throughput test using only ETH transfers at a sustained 5 Mgas/s. Three phases: warmup (30s at 1 Mgas/s), steady state (5 min at 5 Mgas/s), and cooldown (30s at 500 Kgas/s).

This is the simplest scenario and a good starting point for validating that the pipeline works.

## Mixed Workload

**File:** `scenarios/mixed-workload.yaml`

Tests with multiple transaction templates to simulate realistic usage:

| Template | Weight | Estimated Gas |
|----------|--------|--------------|
| simple-transfer | 30% | 21,000 |
| erc20-transfer | 25% | ~65,000 |
| erc20-approve | 10% | ~46,000 |
| storage-write | 20% | ~44,000 |
| compute-heavy | 15% | ~200,000 |

The mixed workload produces a more realistic gas profile than pure transfers.

## Ramp to Saturation

**File:** `scenarios/ramp-to-saturation.yaml`

Five phases that progressively increase the gas rate from 1 Mgas/s to 50 Mgas/s. Designed to find the capacity ceiling of the system under test.

The resulting report shows where latency begins to degrade (degradation onset) and where the system becomes unstable (instability threshold). These points define the **safe operating envelope**.

## Fault Tolerance

**File:** `scenarios/fault-tolerance.yaml`

Injects controlled network faults during load:

1. 200ms latency on sequencer RPC at t=60s
2. Feed relay disconnect at t=120s
3. 10% packet loss at t=180s

Each fault is cleared before the next is injected. This tests resilience and recovery behavior.

## Correctness Audit

**File:** `scenarios/correctness-audit.yaml`

Low-rate (500 Kgas/s) test with all oracle invariants enabled:

- Balance conservation
- Nonce monotonicity
- Gas accounting

Designed for correctness verification rather than throughput testing. The low rate ensures the oracle can keep up with verification.

## Quick Smoke Test

**File:** `scenarios/quick-smoke-test.yaml`

Minimal 2-minute test at 1 Mgas/s with a single warmup phase. Used for fast validation that the full pipeline works end-to-end.
