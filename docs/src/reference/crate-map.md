# Crate Map

GasHammer is organized as a Cargo workspace with 12 crates. Each crate has a single responsibility and well-defined dependency boundaries.

## Dependency Graph

```text
                        gashammer-common
                       /    |    |    \
                      /     |    |     \
              nitro  work  tele  scenario  oracle  fault  report  docgen  testenv
               |      |     |       |        |       |      |       |       |
               edge   |     |       |        |       |      |       |       |
               |      |     |       |        |       |      |       |       |
               hive ──┘     |       |        |       |      |       |       |
                            |       |        |       |      |       |       |
                            └───────┴────────┴───────┴──────┘       |       |
                                            |                       |       |
                                       (all share common)           |       |
```

**Rule:** All crates depend on `gashammer-common`. No crate depends on a binary crate. The `edge` crate depends on `nitro` and `workload`. The `hive` crate depends on `common` only.

## Crate Details

### gashammer-common

Shared types, configuration, build info, and the error code registry.

| Export | Description |
|--------|-------------|
| `RunId`, `EdgeId`, `ScenarioId`, `PhaseId` | UUID newtype wrappers |
| `GasRate`, `TxCount` | Numeric newtype wrappers |
| `ErrorCode`, `ErrorCategory` | Structured error codes |
| `BuildInfo`, `GASHAMMER_CANARY` | Build metadata and DNA |
| `gashammer::v1::*` | Generated gRPC/protobuf types |

### gashammer-nitro

Nitro protocol adapters. Wire-protocol-only --- never imports Nitro crates.

| Export | Description |
|--------|-------------|
| `NitroRpcProvider` | JSON-RPC provider with retry |
| `FeedConsumer`, `FeedConfig` | WebSocket feed consumer |
| `L1ContractReader`, `L1Config` | SequencerInbox and RollupCore reader |
| `L1Monitor`, `L1Event` | Background L1 polling task |
| `NitroError` | Nitro-specific errors (E100--E102) |

### gashammer-edge

Edge worker runtime and binary.

| Export | Description |
|--------|-------------|
| `EdgeRuntime`, `RuntimeState` | Edge lifecycle state machine |
| `EdgeConfig` | Edge configuration |
| `TxPipeline` | Transaction submission pipeline |
| `FeedCorrelator` | Feed-to-tx correlation |
| Binary: `gashammer-edge` | Edge entry point with CLI |

### gashammer-hive

Hive control plane and binary.

| Export | Description |
|--------|-------------|
| `RunOrchestrator`, `RunState` | Run lifecycle state machine |
| `EdgeRegistry`, `EdgeStatus` | Edge tracking with reaper |
| `AppState`, `router()` | axum REST API |
| Binary: `gashammer-hive` | Hive entry point with CLI |

### gashammer-workload

Workload generation engine.

| Export | Description |
|--------|-------------|
| `WorkloadEngine`, `EngineConfig` | Core engine with rate control |
| `GasRateMode` | Sustained, Ramped, Bursty, Custom |
| `Parameterizer` trait | Template interface |
| `AccountPool`, `Account` | Deterministic HD wallet accounts |
| `builtin_templates()` | 8 built-in tx templates |

### gashammer-telemetry

Event model and transport.

| Export | Description |
|--------|-------------|
| `TelemetryEvent`, `EventPayload` | Event types |
| `PipelineConfig`, `PipelineMetrics` | Pipeline configuration |

### gashammer-oracle

Correctness verification.

| Export | Description |
|--------|-------------|
| `Invariant` trait | Check interface |
| `Verdict`, `Evidence` | Check results |
| `CheckSummary`, `CorrectnessVerdict` | Aggregated results |
| `Observation` | Simplified event for checks |

### gashammer-fault

Fault injection.

| Export | Description |
|--------|-------------|
| `FaultAdapter` trait | Adapter interface |
| `FaultManager` | Routing and lifecycle |
| `NetemAdapter` | tc netem wrapper |
| `IptablesAdapter` | iptables wrapper |
| `FaultTimeline` | Scheduled fault events |

### gashammer-report

Report generation.

| Export | Description |
|--------|-------------|
| `RunReport` | Complete run report |
| `CapacityEnvelope` | Safe operating envelope |
| `BottleneckClassification` | Bottleneck identification |
| Regression detection | Baseline comparison |

### gashammer-scenario

Scenario Definition Language.

| Export | Description |
|--------|-------------|
| `ScenarioDefinition` | Parsed SDL structure |
| `validate()` | Scenario validation |
| Compiler | SDL to run configuration |

### gashammer-docgen

Documentation engine.

| Export | Description |
|--------|-------------|
| `parse_source()` | Rust source file parser (via `syn`) |
| `ModuleDoc`, `ItemDoc` | Documentation model |

### gashammer-testenv

Testcontainers-based Nitro devnet.

| Export | Description |
|--------|-------------|
| `NitroDevnet` | Devnet orchestrator (Minimal/Standard/Full) |
| `LifecycleTest` | End-to-end test builder |
| `TestContracts` | Compiled test contract bytecode |
| Container images | L1 Geth, Sequencer, Feed Relay |
