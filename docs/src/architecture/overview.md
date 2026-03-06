# System Overview

GasHammer is structured as a distributed system with a central control plane (**hive**) and regional workers (**edges**). This architecture allows scaling load generation across multiple machines while maintaining centralized orchestration and data collection.

## Design Principles

1. **Wire-protocol-only integration** --- GasHammer never imports Nitro crate code. All interaction is via JSON-RPC, WebSocket feed, gRPC, and L1 contract reads.
2. **Gas-first workload modeling** --- workloads are defined by target gas/sec, not requests/sec. This accurately models blockchain resource consumption.
3. **Deterministic reproducibility** --- seeded PRNGs ensure identical test runs for reliable comparison.
4. **Correctness by default** --- every run can verify invariants, not just measure throughput.

## Crate Architecture

```text
gashammer/
├── gashammer-common       Shared types, config, build info, error codes
├── gashammer-nitro        Nitro protocol adapters (RPC, feed, L1 contracts)
├── gashammer-hive         Hive control plane (REST API, gRPC, orchestration)
├── gashammer-edge         Edge runtime (tx pipeline, feed correlator)
├── gashammer-workload     Workload engine (templates, gas modeling, rate control)
├── gashammer-telemetry    Event model, pipeline, Parquet storage
├── gashammer-oracle       Correctness checks (invariants, verdicts)
├── gashammer-fault        Fault injection (network faults, timeline, safety)
├── gashammer-report       Reporting (latency, capacity envelope, regression)
├── gashammer-scenario     Scenario Definition Language (parser, validator, compiler)
├── gashammer-docgen       Documentation engine (syn parser, mdBook generation)
└── gashammer-testenv      Testcontainers Nitro devnet orchestration
```

## Component Interactions

```text
                          ┌──────────────────────┐
                          │        Hive           │
                          │                       │
                          │  ┌─────────────────┐  │
      REST API ──────────▶│  │  Orchestrator   │  │
    (scenarios,           │  │  (run lifecycle, │  │
     runs, edges)         │  │   phase sync)   │  │
                          │  └────────┬────────┘  │
                          │           │ gRPC      │
                          │  ┌────────┴────────┐  │
                          │  │  Edge Registry   │  │
                          │  │  (heartbeat,     │  │
                          │  │   health check)  │  │
                          │  └────────┬────────┘  │
                          │           │           │
                          │  ┌────────┴────────┐  │
                          │  │  Telemetry Sink  │  │
                          │  │  (Parquet files) │  │
                          │  └────────┬────────┘  │
                          │           │           │
                          │  ┌────────┴────────┐  │
                          │  │  Report Engine   │  │
                          │  │  (latency, SOE,  │  │
                          │  │   regression)    │  │
                          │  └─────────────────┘  │
                          └───────────────────────┘
                                    │
                                    │ gRPC (mTLS)
                 ┌──────────────────┼──────────────────┐
                 │                  │                   │
           ┌─────┴─────┐     ┌─────┴─────┐      ┌─────┴─────┐
           │   Edge 1   │     │   Edge 2   │      │   Edge N   │
           │            │     │            │      │            │
           │ ┌────────┐ │     │ ┌────────┐ │      │ ┌────────┐ │
           │ │Workload│ │     │ │Workload│ │      │ │Workload│ │
           │ │ Engine │ │     │ │ Engine │ │      │ │ Engine │ │
           │ └───┬────┘ │     │ └───┬────┘ │      │ └───┬────┘ │
           │     │      │     │     │      │      │     │      │
           │ ┌───┴────┐ │     │ ┌───┴────┐ │      │ ┌───┴────┐ │
           │ │  Tx    │ │     │ │  Tx    │ │      │ │  Tx    │ │
           │ │Pipeline│ │     │ │Pipeline│ │      │ │Pipeline│ │
           │ └───┬────┘ │     │ └───┬────┘ │      │ └───┬────┘ │
           └─────┼──────┘     └─────┼──────┘      └─────┼──────┘
                 │                  │                    │
                 └──────────────────┼────────────────────┘
                                    │
                              ┌─────┴──────┐
                              │   Nitro     │
                              │  Rollup     │
                              │ (Sequencer, │
                              │  Gateway,   │
                              │  Feed)      │
                              └────────────┘
```

## Data Flow

1. User submits a **scenario** to the hive via REST API
2. Hive validates and compiles the scenario, then creates a **run**
3. Hive distributes the run configuration to registered **edges** via gRPC
4. Each edge starts its **workload engine**, which generates transactions at the target gas rate
5. The **tx pipeline** signs and submits transactions to the Nitro sequencer
6. The **feed correlator** matches submitted txs against the sequencer feed
7. **Telemetry events** flow from edges back to the hive via gRPC streaming
8. The hive writes events to **Parquet files** for durable storage
9. The **correctness oracle** evaluates invariants during and after the run
10. The **report engine** generates the final report with latency analysis, capacity envelope, and verdicts
