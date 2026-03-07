# GasHammer

**Adversarial load-testing and correctness-verification platform for Arbitrum Nitro rollup infrastructure.**

GasHammer is designed to stress-test Nitro rollup deployments by generating realistic, gas-modeled transaction workloads and verifying correctness invariants under load. It interacts with Nitro exclusively through wire protocols (JSON-RPC, WebSocket feed, gRPC) without importing any Nitro internals.

## Key Capabilities

- **Gas-first workload modeling** --- workloads are defined by target gas/sec, not requests/sec. Transaction templates specify gas profiles that accurately model real-world usage patterns.
- **Deterministic reproducibility** --- seeded PRNGs ensure that the same scenario configuration produces the same transaction sequence every time.
- **Correctness verification** --- an oracle checks invariants (balance conservation, nonce monotonicity, gas accounting) both during and after each run.
- **Fault injection** --- controlled network faults (latency, packet loss, partition) test resilience under adverse conditions.
- **Decision-grade reporting** --- capacity envelopes, regression detection, and release gate verdicts provide actionable results.

## Architecture at a Glance

GasHammer uses a **hive/edge topology**:

```text
                    ┌──────────────┐
                    │     Hive     │  Control plane (1 instance)
                    │  REST + gRPC │  Orchestrates runs, collects telemetry
                    └──────┬───────┘
                           │ gRPC
              ┌────────────┼────────────┐
              │            │            │
        ┌─────┴─────┐┌────┴──────┐┌────┴──────┐
        │   Edge 1  ││  Edge 2   ││  Edge N   │  Workers (many instances)
        │  Region A ││ Region B  ││ Region C  │  Generate txs, collect data
        └─────┬─────┘└────┬──────┘└────┬──────┘
              │           │            │
              └───────────┼────────────┘
                          │ JSON-RPC / WebSocket
                    ┌─────┴──────┐
                    │   Nitro    │  System under test
                    │  Rollup   │
                    └────────────┘
```

- **Hive** accepts scenarios via REST API, distributes work to edges, synchronizes phases, and aggregates results.
- **Edges** connect to the Nitro sequencer, generate transactions at the target gas rate, and stream telemetry back to the hive.

## License

GasHammer is licensed under the Business Source License 1.1 (BSL-1.1).

## Next Steps

- [Getting Started](./getting-started/index.md) --- install, configure, and run your first test
- [Architecture Overview](./architecture/overview.md) --- understand the system design
- [Scenario Authoring](./scenarios/index.md) --- write test scenarios in SDL
