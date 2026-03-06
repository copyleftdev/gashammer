# RFC-0002: Technology Stack & Dependencies

## Status

Draft

## Author

Don Johnson

## Depends On

RFC-0001 (Nitro Integration Surface)

## Summary

This RFC locks down the language, crate ecosystem, build system, container strategy, and deployment model for GasHammer. Every dependency is justified against the integration boundaries defined in RFC-0001.

---

## 1. Language Decision: Rust

### 1.1 Rationale

GasHammer is an **external harness** that interacts with Arbitrum Nitro exclusively through wire protocols (RFC-0001 §1). It does not link against, fork, or import Nitro node code. This makes the language choice independent of Nitro's own stack.

**Why Rust over Go:**

| Concern | Rust | Go |
| --- | --- | --- |
| **Concurrency model** | `tokio` async — millions of concurrent tasks on a single runtime; zero-cost futures | Goroutines — lightweight but GC pauses under high allocation pressure |
| **Memory control** | No GC; deterministic allocation; critical for latency-sensitive measurement code | GC pauses introduce measurement noise in microsecond-precision timing |
| **Type safety** | Algebraic types, exhaustive pattern matching, `Result`/`Option` — eliminates entire classes of silent failures | Interface-based; errors are values but easy to ignore |
| **Performance ceiling** | Zero-cost abstractions; SIMD, inline assembly available when needed | Good throughput but bounded by runtime overhead |
| **Ecosystem fit** | `alloy` is the standard Ethereum library; `revm` for local EVM simulation; `tokio-tungstenite` for WebSocket | `go-ethereum` is mature but pulls in Nitro's dependency graph |
| **Signal to Offchain Labs** | Demonstrates systems-level proficiency; Nitro uses Rust for its most critical paths (fraud proofs, WASM, Stylus VM) | Would blend in rather than differentiate |
| **Plugin safety** | Trait-based plugin system with compile-time guarantees | Interface-based plugins; runtime panics possible |

**Acknowledged trade-offs:**

- Go has faster compile times
- Go has a shallower learning curve for contributors
- Go's `go-ethereum` bindings are more battle-tested than `alloy` for some edge cases

These trade-offs are acceptable. GasHammer is a specialized systems tool, not a general-purpose service.

### 1.2 Minimum Supported Rust Version (MSRV)

**MSRV: 1.75.0** (stable async trait support via `impl Trait` in trait methods begins at 1.75)

In practice, target the latest stable toolchain. Pin via `rust-toolchain.toml`:

```toml
[toolchain]
channel = "stable"
components = ["rustfmt", "clippy"]
```

---

## 2. Workspace Structure

GasHammer uses a Cargo workspace with one crate per architectural component:

```
gashammer/
├── Cargo.toml              # workspace root
├── Cargo.lock
├── rust-toolchain.toml
├── deny.toml               # cargo-deny config
├── prd.md
├── rfcs/
│   ├── RFC-0001-nitro-integration-surface.md
│   ├── RFC-0002-technology-stack.md
│   └── ...
├── crates/
│   ├── gashammer-hive/     # control plane service
│   ├── gashammer-edge/     # regional worker binary
│   ├── gashammer-scenario/ # scenario compiler + validator
│   ├── gashammer-oracle/   # correctness verification engine
│   ├── gashammer-report/   # reporting + regression engine
│   ├── gashammer-proto/    # shared protobuf/types for hive↔edge
│   ├── gashammer-telemetry/# metrics, tracing, event model
│   ├── gashammer-nitro/    # Nitro-specific adapters (RPC, feed, contracts)
│   ├── gashammer-fault/    # fault injection framework
│   └── gashammer-common/   # shared types, config, error handling
├── contracts/              # Solidity ABIs / interface definitions
├── scenarios/              # example scenario YAML files
├── deploy/
│   ├── docker/             # Dockerfiles
│   └── helm/               # Helm charts
└── tests/
    ├── integration/        # cross-crate integration tests
    └── fixtures/           # test data, mock feeds, sample scenarios
```

### 2.1 Crate Dependency Graph

```
gashammer-common ◄─── (all crates depend on this)
       ▲
       │
gashammer-proto ◄──── gashammer-hive
       ▲               ▲
       │               │
gashammer-nitro ◄─── gashammer-edge
       ▲               ▲
       │               │
gashammer-telemetry    gashammer-fault
       ▲
       │
gashammer-scenario ◄── gashammer-oracle
                        ▲
                        │
                   gashammer-report
```

Key constraint: **no circular dependencies**. The `gashammer-common` crate holds shared types. The `gashammer-proto` crate holds wire format definitions. The `gashammer-nitro` crate encapsulates all Arbitrum-specific protocol logic so the rest of the system is chain-agnostic in principle.

---

## 3. Core Dependencies

### 3.1 Ethereum / Blockchain Interaction

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `alloy` | `^0.9` | Ethereum primitives, ABI encoding, transaction construction, signing, JSON-RPC provider | `gashammer-nitro`, `gashammer-edge` |
| `alloy-provider` | (part of alloy) | HTTP JSON-RPC transport; connection pooling | `gashammer-nitro` |
| `alloy-signer-local` | (part of alloy) | Local private key signing for test wallets | `gashammer-edge` |
| `alloy-sol-types` | (part of alloy) | Solidity type encoding/decoding for contract interaction | `gashammer-nitro`, `gashammer-oracle` |
| `alloy-rpc-types` | (part of alloy) | RPC request/response types | `gashammer-nitro` |

**Why `alloy` over `ethers-rs`:** `ethers-rs` is deprecated. `alloy` is its successor, maintained by the same team (Paradigm/Alloy contributors), and is the Rust Ethereum ecosystem standard.

### 3.2 Async Runtime & Networking

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `tokio` | `^1.36` | Async runtime (multi-threaded scheduler) | All crates |
| `tokio-tungstenite` | `^0.24` | WebSocket client for sequencer feed subscription | `gashammer-nitro` |
| `reqwest` | `^0.12` | HTTP client (built on hyper + tokio); used where alloy provider is insufficient | `gashammer-nitro`, `gashammer-telemetry` |
| `axum` | `^0.7` | HTTP framework for hive API server | `gashammer-hive` |
| `tonic` | `^0.12` | gRPC framework for hive↔edge coordination | `gashammer-hive`, `gashammer-edge`, `gashammer-proto` |
| `prost` | `^0.13` | Protobuf code generation for gRPC messages | `gashammer-proto` |
| `tower` | `^0.5` | Middleware (timeouts, rate limiting, retries) for axum and tonic services | `gashammer-hive` |

### 3.3 Serialization & Configuration

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `serde` | `^1.0` | Serialization framework | All crates |
| `serde_json` | `^1.0` | JSON ser/de | All crates |
| `serde_yaml` | `^0.9` | YAML parsing for scenario definitions | `gashammer-scenario` |
| `jsonschema` | `^0.22` | JSON Schema validation for scenario specs | `gashammer-scenario` |
| `toml` | `^0.8` | TOML config file parsing | `gashammer-common` |
| `config` | `^0.14` | Layered configuration (file + env + CLI) | `gashammer-common` |
| `clap` | `^4.5` | CLI argument parsing | `gashammer-hive`, `gashammer-edge` |

### 3.4 Observability

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `tracing` | `^0.1` | Structured logging and span instrumentation | All crates |
| `tracing-subscriber` | `^0.3` | Log output formatting (JSON, pretty) | `gashammer-hive`, `gashammer-edge` |
| `tracing-opentelemetry` | `^0.27` | OpenTelemetry bridge for distributed tracing | `gashammer-telemetry` |
| `opentelemetry` | `^0.27` | Telemetry API | `gashammer-telemetry` |
| `opentelemetry-otlp` | `^0.27` | OTLP exporter (traces + metrics) | `gashammer-telemetry` |
| `prometheus-client` | `^0.23` | Prometheus metrics exposition for GasHammer's own metrics | `gashammer-telemetry` |
| `metrics` | `^0.24` | Lightweight metrics facade | `gashammer-telemetry` |

### 3.5 Time & Synchronization

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `quanta` | `^0.12` | High-resolution monotonic clock (TSC-based on x86) | `gashammer-edge`, `gashammer-telemetry` |
| `chrono` | `^0.4` | Wall clock timestamps, formatting | All crates |
| `tokio` (time) | (included) | Async timers, intervals, timeouts | All crates |

**Why `quanta`:** Monotonic clock with nanosecond resolution and low overhead. Critical for accurate latency measurement in edges where `std::time::Instant` may have platform-dependent resolution. Falls back gracefully on non-x86.

### 3.6 Cryptography & Signing

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `alloy-signer-local` | (part of alloy) | secp256k1 signing for Ethereum transactions | `gashammer-edge` |
| `rand` | `^0.8` | PRNG for deterministic workload generation (with fixed seeds) | `gashammer-edge` |
| `rand_chacha` | `^0.3` | ChaCha20-based PRNG for reproducible sequences | `gashammer-edge` |
| `rustls` | `^0.23` | TLS for hive↔edge mTLS communication | `gashammer-hive`, `gashammer-edge` |
| `rcgen` | `^0.13` | Test certificate generation | `tests/` |

### 3.7 Data Storage

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `sqlx` | `^0.8` | Async PostgreSQL driver for run metadata, edge registry | `gashammer-hive` |
| `object_store` | `^0.11` | Abstraction over S3/local/GCS for artifact storage | `gashammer-hive`, `gashammer-report` |
| `parquet` | `^53.0` (via `arrow`) | Columnar format for time-series metric export | `gashammer-report` |

### 3.8 Testing

| Crate | Version | Purpose | Used By |
| --- | --- | --- | --- |
| `tokio::test` | (included) | Async test runtime | All crates |
| `proptest` | `^1.5` | Property-based testing for scenario compilation, workload generation | `gashammer-scenario`, `gashammer-edge` |
| `wiremock` | `^0.6` | HTTP mock server for RPC endpoint simulation | `tests/integration/` |
| `testcontainers` | `^0.23` | Docker-based test fixtures (PostgreSQL, mock Nitro nodes) | `tests/integration/` |
| `criterion` | `^0.5` | Microbenchmarks for hot paths (tx construction, feed parsing) | `benches/` |
| `insta` | `^1.39` | Snapshot testing for report output, scenario compilation | `gashammer-report`, `gashammer-scenario` |

---

## 4. Wire Protocol: Hive ↔ Edge Communication

### 4.1 Protocol Choice: gRPC over mTLS

Hive-to-edge communication uses **gRPC** (via `tonic`) over **mutual TLS**:

- **Bidirectional streaming** — edges stream telemetry events to hive; hive streams control messages (phase changes, barriers) to edges
- **Protobuf** — strongly typed, compact, versionable wire format
- **mTLS** — mutual authentication; edges present certificates signed by hive CA
- **HTTP/2** — multiplexing, flow control, built into gRPC

### 4.2 Why gRPC Over Alternatives

| Alternative | Rejection Reason |
| --- | --- |
| REST/HTTP | No bidirectional streaming; polling is wasteful for real-time coordination |
| WebSocket | Less structured; no built-in schema evolution; harder to generate typed clients |
| NATS/Kafka | Adds infrastructure dependency; overkill for control plane; appropriate for telemetry pipeline later |
| Custom TCP | Reinventing wheels; maintenance burden |

### 4.3 Proto Package Structure

```
gashammer-proto/
├── proto/
│   ├── gashammer/
│   │   ├── v1/
│   │   │   ├── hive.proto        # Hive service definition
│   │   │   ├── edge.proto        # Edge registration, heartbeat
│   │   │   ├── scenario.proto    # Scenario distribution
│   │   │   ├── telemetry.proto   # Event and metric streaming
│   │   │   ├── barrier.proto     # Synchronization primitives
│   │   │   └── common.proto      # Shared types (region, timestamp, etc.)
│   │   └── ...
├── build.rs                       # prost/tonic codegen
└── src/
    └── lib.rs                     # Re-exports generated types
```

---

## 5. External API: Hive HTTP API

The hive exposes a **REST API** (via `axum`) for human/CI interaction:

- Scenario submission and validation
- Run lifecycle (start, pause, abort, status)
- Edge fleet status
- Report retrieval
- Artifact download

This is separate from the gRPC coordination plane. REST is appropriate here because the consumers are dashboards, CLI tools, and CI pipelines — not real-time edge coordination.

### 5.1 API Versioning

Prefix all routes with `/api/v1/`. Breaking changes require a new version prefix.

---

## 6. Build System

### 6.1 Cargo

Standard `cargo build`, `cargo test`, `cargo clippy`, `cargo fmt`. No custom build tools.

### 6.2 Protobuf Compilation

Handled by `prost-build` + `tonic-build` in `gashammer-proto/build.rs`. Requires `protoc` installed (documented in README, provided in CI image and Dockerfile).

### 6.3 Linting & Quality Gates

| Tool | Purpose | Enforcement |
| --- | --- | --- |
| `cargo fmt --check` | Formatting | CI gate (fail on diff) |
| `cargo clippy -- -D warnings` | Lint | CI gate (deny all warnings) |
| `cargo deny check` | License + vulnerability audit | CI gate |
| `cargo test` | Unit + integration tests | CI gate |
| `cargo bench` | Performance regression detection | CI report (not gate in MVP) |

### 6.4 cargo-deny Configuration

```toml
# deny.toml
[advisories]
vulnerability = "deny"
unmaintained = "warn"

[licenses]
allow = ["MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unicode-3.0"]
confidence-threshold = 0.8

[bans]
multiple-versions = "warn"
wildcards = "deny"

[sources]
unknown-registry = "deny"
unknown-git = "deny"
```

---

## 7. Container Strategy

### 7.1 Multi-Stage Build

```dockerfile
# Stage 1: Build
FROM rust:1.82-bookworm AS builder
WORKDIR /build
COPY . .
RUN cargo build --release --bin gashammer-hive --bin gashammer-edge

# Stage 2: Runtime
FROM gcr.io/distroless/cc-debian12:nonroot
COPY --from=builder /build/target/release/gashammer-hive /usr/local/bin/
COPY --from=builder /build/target/release/gashammer-edge /usr/local/bin/
```

**Separate images** for hive and edge — different binaries, different resource profiles, different deployment targets.

### 7.2 Image Strategy

| Image | Base | Size Target | Deployed To |
| --- | --- | --- | --- |
| `gashammer-hive` | `distroless/cc` | <50MB | Central K8s cluster |
| `gashammer-edge` | `distroless/cc` | <40MB | Regional K8s / VM / bare metal |

### 7.3 Why Distroless

- No shell, no package manager — reduced attack surface
- Aligns with Offchain Labs security posture (RFC-0001 §14: mTLS, scoped permissions)
- Non-root by default

---

## 8. Deployment Model

### 8.1 Kubernetes (Primary)

**Hive control plane:** Kubernetes Deployment + Service in a central cluster.

- StatefulSet if PostgreSQL is co-located (prefer managed RDS in production)
- Horizontal scaling: single leader for coordination; read replicas for API queries

**Edge workers:** Deployable to:

1. **K8s clusters in target regions** — Deployment per region, scaled by scenario demand
2. **VMs** — systemd unit running the edge binary directly
3. **Bare metal** — same as VM; binary + config + certs

### 8.2 Helm Charts

```
deploy/helm/
├── gashammer-hive/
│   ├── Chart.yaml
│   ├── values.yaml
│   └── templates/
│       ├── deployment.yaml
│       ├── service.yaml
│       ├── configmap.yaml
│       └── secret.yaml
├── gashammer-edge/
│   ├── Chart.yaml
│   ├── values.yaml
│   └── templates/
│       ├── deployment.yaml
│       ├── configmap.yaml
│       └── secret.yaml
└── gashammer-infra/
    ├── Chart.yaml
    └── templates/
        ├── postgresql.yaml   # optional: use managed DB in prod
        └── s3-bucket.yaml    # artifact storage
```

Helm charts align with Offchain Labs' documented deployment pattern (they provide Helm charts for HA sequencer setup).

### 8.3 Cloud Provider: AWS (Primary)

| Service | GasHammer Use |
| --- | --- |
| **EKS** | K8s clusters for hive and regional edges |
| **RDS (PostgreSQL)** | Run metadata, edge registry, scenario history |
| **S3** | Artifact storage (run results, telemetry dumps, report bundles) |
| **ECR** | Container image registry |
| **CloudWatch / Grafana** | Operational monitoring of GasHammer itself |
| **Route 53** | DNS for hive endpoint; edge discovery |

AWS is chosen because Offchain Labs job listings and infrastructure docs indicate AWS as their primary cloud.

### 8.4 Edge Bootstrap Flow

1. Edge binary starts with: hive endpoint URL + bootstrap token + region label
2. Edge presents bootstrap token to hive over mTLS
3. Hive validates token, issues edge certificate (short-lived)
4. Edge registers: region, capacity class, supported adapters, software version
5. Edge enters heartbeat loop
6. Hive assigns scenario slices when a run starts

---

## 9. Configuration Model

### 9.1 Layered Configuration

Resolution order (later overrides earlier):

1. Compiled defaults
2. Config file (`gashammer.toml`)
3. Environment variables (`GASHAMMER_*`)
4. CLI flags

### 9.2 Hive Config Example

```toml
[hive]
listen_addr = "0.0.0.0:8080"          # REST API
grpc_addr = "0.0.0.0:9090"            # gRPC coordination
max_edges = 100

[hive.tls]
cert_file = "/etc/gashammer/tls/hive.crt"
key_file = "/etc/gashammer/tls/hive.key"
ca_file = "/etc/gashammer/tls/ca.crt"

[storage.postgres]
url = "postgres://gashammer:***@rds-host:5432/gashammer"

[storage.artifacts]
backend = "s3"
bucket = "gashammer-artifacts"
region = "us-east-1"

[telemetry]
otlp_endpoint = "http://otel-collector:4317"
prometheus_port = 9091
log_format = "json"
```

### 9.3 Edge Config Example

```toml
[edge]
region = "us-west-2"
capacity_class = "large"
hive_grpc_addr = "hive.gashammer.internal:9090"

[edge.tls]
cert_file = "/etc/gashammer/tls/edge.crt"
key_file = "/etc/gashammer/tls/edge.key"
ca_file = "/etc/gashammer/tls/ca.crt"

[edge.target]
sequencer_rpc = "https://sequencer.staging.internal/rpc"
gateway_rpc = "https://rpc.staging.internal/rpc"
feed_relay = "ws://feed-relay.staging.internal:9642"
chain_id = 421614

[telemetry]
otlp_endpoint = "http://otel-collector:4317"
log_format = "json"
```

### 9.4 Secrets Management

- **Never in config files or scenario YAML** (RFC-0001 §14, PRD §11 Security)
- Private keys for test wallets: injected via environment variable or mounted secret volume
- Bootstrap tokens: short-lived, single-use
- TLS certificates: mounted from K8s Secret or Vault

---

## 10. Error Handling Strategy

### 10.1 Error Type Hierarchy

```rust
// gashammer-common/src/error.rs

#[derive(Debug, thiserror::Error)]
pub enum GashammerError {
    #[error("RPC error: {0}")]
    Rpc(#[from] alloy::transports::TransportError),

    #[error("Feed error: {0}")]
    Feed(String),

    #[error("Scenario validation error: {0}")]
    ScenarioValidation(String),

    #[error("Edge communication error: {0}")]
    EdgeComm(#[from] tonic::Status),

    #[error("Storage error: {0}")]
    Storage(String),

    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Timeout: {context}")]
    Timeout { context: String },

    #[error("Invariant violation: {0}")]
    Invariant(String),
}
```

### 10.2 Principles

- **No silent failure** — all errors propagated or logged with context
- **`Result<T, GashammerError>`** as the return type for all fallible operations
- **`anyhow`** for binary entry points (hive, edge); **`thiserror`** for library crates
- **Structured error logging** via `tracing::error!` with span context

---

## 11. Testing Strategy

### 11.1 Test Pyramid

| Level | Tool | Scope | Run Frequency |
| --- | --- | --- | --- |
| **Unit** | `cargo test` | Per-function logic, parsing, validation | Every commit |
| **Property** | `proptest` | Scenario compilation invariants, workload generation determinism | Every commit |
| **Integration** | `testcontainers` + `wiremock` | Hive↔edge communication, RPC mock interaction | Every PR |
| **Benchmark** | `criterion` | Tx construction throughput, feed parsing rate | Weekly / release |
| **Snapshot** | `insta` | Report format stability, scenario compilation output | Every PR |
| **End-to-end** | Docker Compose + local Nitro devnet | Full scenario execution against real Nitro node | Pre-release |

### 11.2 Determinism Testing

Since GasHammer claims reproducible workloads (PRD §10.3), testing must verify:

- Same seed + same scenario → identical transaction sequence
- Same seed + different edge count → equivalent aggregate workload (partitioned differently)
- Timestamp-dependent code uses injected clocks in tests

### 11.3 Local Nitro Devnet

For end-to-end testing, use Offchain Labs' Docker-based local devnet:

```bash
# From OffchainLabs/nitro (docker-compose based)
docker compose up -d
```

This provides a local sequencer, batch poster, validator, and L1 (Geth dev mode). GasHammer integration tests target this.

---

## 12. Risks

### Risk: alloy API instability

`alloy` is pre-1.0 and evolving. Breaking changes between minor versions are possible.

**Mitigation:** Pin exact versions in `Cargo.lock`. Monitor alloy releases. Isolate alloy usage behind the `gashammer-nitro` adapter crate so breakage is contained.

### Risk: Protobuf schema evolution

gRPC message changes can break hive↔edge compatibility during rolling upgrades.

**Mitigation:** Follow protobuf best practices — additive-only field changes, reserved field numbers, version negotiation at connection time.

### Risk: Distroless debugging difficulty

Distroless images lack shells for runtime debugging.

**Mitigation:** Provide a debug image variant with shell. Use structured logging and OTLP tracing as the primary debugging mechanism. K8s ephemeral containers for emergency access.

### Risk: Cross-compilation complexity for multi-arch edges

Edges may run on ARM (Graviton) or x86.

**Mitigation:** CI builds both `linux/amd64` and `linux/arm64` via `cross` or Docker buildx. Test both in CI.

### Risk: quanta TSC dependency on non-x86

`quanta` uses TSC on x86 for high-resolution timing. On ARM, it falls back to `clock_gettime`.

**Mitigation:** Acceptable — ARM fallback is still nanosecond-resolution. Document that x86 edges provide highest timing fidelity.

---

## 13. Open Questions

1. Should `gashammer-nitro` eventually support a second chain adapter (e.g., OP Stack) or remain Arbitrum-only?
2. Is PostgreSQL the right metadata store, or should we use SQLite for simpler single-node hive deployments?
3. Should the hive API support GraphQL in addition to REST for richer query patterns?
4. What is the minimum viable Helm chart for the interview deliverable — full production-grade or functional demo?
5. Should we vendor the `alloy` dependency to insulate from upstream breaking changes?

---

## 14. References

- [alloy — Rust Ethereum SDK](https://github.com/alloy-rs/alloy)
- [tokio — Async runtime](https://tokio.rs/)
- [tonic — gRPC for Rust](https://github.com/hyperium/tonic)
- [axum — Web framework](https://github.com/tokio-rs/axum)
- [cargo-deny — Supply chain security](https://github.com/EmbarkStudios/cargo-deny)
- [Offchain Labs Nitro Docker Compose](https://github.com/OffchainLabs/nitro/tree/master/docker-compose)
- [Offchain Labs Helm Charts (HA Sequencer)](https://docs.arbitrum.io/launch-arbitrum-chain/arbitrum-node-runners/high-availability-sequencer-docs)
- [Distroless Container Images](https://github.com/GoogleContainerTools/distroless)
