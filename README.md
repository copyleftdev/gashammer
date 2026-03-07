# GasHammer

**Adversarial load-testing and correctness-verification platform for Arbitrum Nitro rollup infrastructure.**

[![CI](https://github.com/copyleftdev/gashammer/actions/workflows/ci.yml/badge.svg)](https://github.com/copyleftdev/gashammer/actions/workflows/ci.yml)
[![Docs](https://github.com/copyleftdev/gashammer/actions/workflows/docs.yml/badge.svg)](https://copyleftdev.github.io/gashammer/)
[![License: BSL-1.1](https://img.shields.io/badge/License-BSL--1.1-blue.svg)](LICENSE)

GasHammer stress-tests Nitro rollup deployments by generating realistic, gas-modeled transaction workloads and verifying correctness invariants under load. It interacts with Nitro exclusively through wire protocols (JSON-RPC, WebSocket feed, gRPC) — no internal linking against Nitro.

## Key Capabilities

- **Gas-first workload modeling** — workloads defined by target gas/sec, not requests/sec
- **Deterministic reproducibility** — seeded PRNGs produce identical transaction sequences
- **Correctness verification** — oracle checks invariants (balance conservation, nonce monotonicity, gas accounting)
- **Fault injection** — controlled network faults via tc netem and iptables
- **Capacity envelope** — find the safe operating range with ramp-to-saturation scenarios
- **Regression detection** — compare runs against baselines for release gating

## Architecture

```
                ┌──────────────┐
                │     Hive     │  Control plane
                │  REST + gRPC │
                └──────┬───────┘
                       │
          ┌────────────┼────────────┐
          │            │            │
    ┌─────┴─────┐┌────┴──────┐┌────┴──────┐
    │  Edge A   ││  Edge B   ││  Edge N   │  Workers
    └─────┬─────┘└────┬──────┘└────┬──────┘
          └───────────┼────────────┘
                      │
                ┌─────┴──────┐
                │   Nitro    │  System under test
                └────────────┘
```

## Quick Start

```bash
cargo build --workspace
cargo test --workspace
```

See the [documentation](https://copyleftdev.github.io/gashammer/) for installation, configuration, and scenario authoring guides.

## Workspace Crates

| Crate | Purpose |
|-------|---------|
| `gashammer-common` | Shared types, error codes, build info |
| `gashammer-nitro` | RPC, WebSocket feed, L1 contract readers |
| `gashammer-edge` | Edge worker runtime and binary |
| `gashammer-hive` | Hive control plane and binary |
| `gashammer-workload` | Gas-rate engine, tx templates, account pool |
| `gashammer-telemetry` | Event model, pipeline, Parquet storage |
| `gashammer-oracle` | Correctness invariants and verdicts |
| `gashammer-fault` | Fault injection (netem, iptables) |
| `gashammer-report` | Capacity envelope, regression detection |
| `gashammer-scenario` | SDL parser, validator, compiler |
| `gashammer-docgen` | Source-to-mdBook documentation engine |
| `gashammer-testenv` | Testcontainers Nitro devnet orchestrator |

## Documentation

Full documentation is available at **[copyleftdev.github.io/gashammer](https://copyleftdev.github.io/gashammer/)**.

Design specifications live in `rfcs/` (RFC-0001 through RFC-0013).

## Acknowledgments

GasHammer targets [Arbitrum Nitro](https://github.com/OffchainLabs/nitro) by [Offchain Labs](https://offchainlabs.com/). Recognition to [@PlasmaPower](https://github.com/PlasmaPower) (Lee Bousfield) and the Offchain Labs engineering team for building Nitro and making the protocol surface well-documented and testable.

## License

Licensed under the [Business Source License 1.1](LICENSE). GasHammer is a registered trademark of Don Johnson.
