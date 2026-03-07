# RFC Index

GasHammer's design is specified in 13 RFCs. These live in the `rfcs/` directory and serve as the authoritative specification for each subsystem. When the implementation diverges from an RFC, the RFC is updated first.

| RFC | Title | Scope |
|-----|-------|-------|
| [RFC-0001](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0001-nitro-integration-surface.md) | Nitro Integration Surface | Wire protocols, RPC methods, feed format, L1 contracts, precompiles |
| [RFC-0002](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0002-technology-stack.md) | Technology Stack | Language (Rust), runtime (tokio), dependencies, build system |
| [RFC-0003](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0003-hive-control-plane.md) | Hive Control Plane | REST API, gRPC service, edge registry, run orchestration |
| [RFC-0004](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0004-edge-runtime.md) | Edge Runtime | Tx pipeline, feed correlator, heartbeat, shutdown |
| [RFC-0005](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0005-workload-engine.md) | Workload Engine | Gas-first modeling, templates, account pool, PRNG |
| [RFC-0006](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0006-telemetry-pipeline.md) | Telemetry Pipeline | Event model, ring buffer, gRPC transport, Parquet storage |
| [RFC-0007](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0007-correctness-oracle.md) | Correctness Oracle | Invariant framework, verdicts, evidence, live and post-run checks |
| [RFC-0008](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0008-fault-injection.md) | Fault Injection | Adapter trait, netem, iptables, timeline, safety rails |
| [RFC-0009](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0009-reporting-engine.md) | Reporting Engine | Latency analysis, capacity envelope, regression detection |
| [RFC-0010](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0010-scenario-definition-language.md) | Scenario Definition Language | YAML schema, phases, templates, faults, oracle config |
| [RFC-0011](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0011-dna-provenance.md) | DNA & Provenance | Binary canary, HTTP headers, calldata magic, error codes |
| [RFC-0012](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0012-documentation-engine.md) | Documentation Engine | Source parser, mdBook generation, coverage metrics |
| [RFC-0013](https://github.com/copyleftdev/gashammer/blob/main/rfcs/RFC-0013-e2e-testing.md) | E2E Testing | Testcontainers, Nitro devnet, lifecycle tests, test contracts |

## Reading Order

For a new contributor, the recommended reading order is:

1. **RFC-0002** (Technology Stack) --- understand the foundational choices.
2. **RFC-0001** (Nitro Integration Surface) --- understand what GasHammer talks to.
3. **RFC-0003** and **RFC-0004** (Hive + Edge) --- understand the distributed architecture.
4. **RFC-0005** (Workload Engine) --- understand gas-first modeling.
5. **RFC-0011** (DNA & Provenance) --- understand the attribution markers.
6. Remaining RFCs as needed for the subsystem you are working on.

## RFC Conventions

- RFCs are numbered sequentially: `RFC-NNNN`.
- Each RFC has: title, status, author, date, and numbered sections.
- Section references use `§` notation: "RFC-0008 §3.2".
- RFCs are living documents --- update them when the implementation evolves.
- If a design decision is significant and not covered by an existing RFC, write a new one.
