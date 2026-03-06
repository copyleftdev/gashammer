---
description: Read all RFCs and generate a complete GitHub backlog — labels, milestones, and issues with acceptance criteria, dependencies, and proper gitflow structure
---

Bootstrap the entire GasHammer project backlog from the RFC corpus. This command reads every RFC, decomposes them into implementable work items, and creates a fully structured GitHub project with labels, milestones, and issues.

**Prerequisites:** `gh` CLI must be installed and authenticated. Verify first.

// turbo
0. `gh auth status 2>&1`
   If not authenticated, STOP and tell the user to run `gh auth login`.

---

## Phase 1: Create Labels

Create the full label taxonomy. Skip any that already exist.

**Scope labels** (crate-level, color: `#1d76db` blue):

```
gh label create "scope:common" --color "1d76db" --description "gashammer-common crate"
gh label create "scope:nitro" --color "1d76db" --description "gashammer-nitro crate (Nitro protocol adapters)"
gh label create "scope:edge" --color "1d76db" --description "gashammer-edge crate (edge runtime)"
gh label create "scope:hive" --color "1d76db" --description "gashammer-hive crate (control plane)"
gh label create "scope:workload" --color "1d76db" --description "gashammer-workload crate (tx templates, gas modeling)"
gh label create "scope:telemetry" --color "1d76db" --description "gashammer-telemetry crate (events, metrics, Parquet)"
gh label create "scope:oracle" --color "1d76db" --description "gashammer-oracle crate (correctness verification)"
gh label create "scope:fault" --color "1d76db" --description "gashammer-fault crate (fault injection)"
gh label create "scope:report" --color "1d76db" --description "gashammer-report crate (reporting engine)"
gh label create "scope:scenario" --color "1d76db" --description "gashammer-scenario crate (SDL parser/compiler)"
gh label create "scope:docgen" --color "1d76db" --description "gashammer-docgen crate (documentation engine)"
gh label create "scope:testenv" --color "1d76db" --description "gashammer-testenv crate (testcontainers devnet)"
gh label create "scope:proto" --color "1d76db" --description "Protobuf definitions (gRPC)"
gh label create "scope:ci" --color "1d76db" --description "CI/CD pipelines and automation"
gh label create "scope:docs" --color "1d76db" --description "Documentation (mdBook, README, guides)"
gh label create "scope:infra" --color "1d76db" --description "Infrastructure (Docker, Helm, Terraform)"
```

**Type labels** (color: `#5319e7` purple):

```
gh label create "type:feature" --color "5319e7" --description "New feature or capability"
gh label create "type:bug" --color "d73a4a" --description "Something is broken"
gh label create "type:refactor" --color "5319e7" --description "Code improvement without behavior change"
gh label create "type:rfc" --color "5319e7" --description "RFC implementation"
gh label create "type:test" --color "5319e7" --description "Test coverage improvement"
gh label create "type:chore" --color "5319e7" --description "Build, CI, dependency, or tooling change"
```

**Priority labels** (color: varies):

```
gh label create "priority:critical" --color "b60205" --description "Must be done before anything else"
gh label create "priority:high" --color "d93f0b" --description "Required for milestone completion"
gh label create "priority:medium" --color "fbca04" --description "Important but not blocking"
gh label create "priority:low" --color "0e8a16" --description "Nice to have, do when time allows"
```

**Status labels** (color: `#c2e0c6` green family):

```
gh label create "status:blocked" --color "b60205" --description "Blocked by another issue or external dependency"
gh label create "status:needs-design" --color "fbca04" --description "Needs design discussion before implementation"
gh label create "status:ready" --color "0e8a16" --description "Ready to be picked up"
```

**RFC reference labels** (color: `#bfd4f2` light blue):

```
gh label create "rfc:0001" --color "bfd4f2" --description "RFC-0001: Nitro Integration Surface"
gh label create "rfc:0002" --color "bfd4f2" --description "RFC-0002: Technology Stack"
gh label create "rfc:0003" --color "bfd4f2" --description "RFC-0003: Hive Control Plane"
gh label create "rfc:0004" --color "bfd4f2" --description "RFC-0004: Edge Runtime"
gh label create "rfc:0005" --color "bfd4f2" --description "RFC-0005: Workload Engine"
gh label create "rfc:0006" --color "bfd4f2" --description "RFC-0006: Telemetry Pipeline"
gh label create "rfc:0007" --color "bfd4f2" --description "RFC-0007: Correctness Oracle"
gh label create "rfc:0008" --color "bfd4f2" --description "RFC-0008: Fault Injection"
gh label create "rfc:0009" --color "bfd4f2" --description "RFC-0009: Reporting Engine"
gh label create "rfc:0010" --color "bfd4f2" --description "RFC-0010: Scenario Definition Language"
gh label create "rfc:0011" --color "bfd4f2" --description "RFC-0011: DNA & Provenance"
gh label create "rfc:0012" --color "bfd4f2" --description "RFC-0012: Documentation Engine"
gh label create "rfc:0013" --color "bfd4f2" --description "RFC-0013: E2E Testing"
```

Run each `gh label create` command. If a label already exists, `gh` will error — that's fine, skip it.

---

## Phase 2: Create Milestones

Create milestones in dependency order. Each milestone represents a shippable increment.

```
gh api repos/{owner}/{repo}/milestones -f title="M0: Project Scaffold" -f description="Cargo workspace, CI pipeline, license headers, DNA provenance foundation. No application logic — just the skeleton that everything builds on." -f due_on="2025-04-01T00:00:00Z"

gh api repos/{owner}/{repo}/milestones -f title="M1: Wire Protocol & Nitro Adapters" -f description="Connect to a real Nitro stack. RPC provider, feed consumer, L1 contract reader. The edge can talk to Nitro but doesn't do anything with it yet." -f due_on="2025-04-15T00:00:00Z"

gh api repos/{owner}/{repo}/milestones -f title="M2: Core Engine" -f description="Hive control plane, edge runtime, workload engine, telemetry pipeline. The system can orchestrate edges, generate transactions, and collect data." -f due_on="2025-05-15T00:00:00Z"

gh api repos/{owner}/{repo}/milestones -f title="M3: Verification & Fault Injection" -f description="Correctness oracle and fault injection framework. The system can verify results and inject controlled failures." -f due_on="2025-06-01T00:00:00Z"

gh api repos/{owner}/{repo}/milestones -f title="M4: Scenarios & Reporting" -f description="Scenario definition language and reporting engine. Users can define tests in YAML and get decision-grade reports." -f due_on="2025-06-15T00:00:00Z"

gh api repos/{owner}/{repo}/milestones -f title="M5: E2E Testing & DevEx" -f description="Testcontainers Nitro devnet, documentation engine, mdBook site. The project is testable against real infrastructure and fully documented." -f due_on="2025-07-01T00:00:00Z"

gh api repos/{owner}/{repo}/milestones -f title="M6: Release Candidate" -f description="Integration testing, performance profiling, security audit, packaging, signed releases. Ship it." -f due_on="2025-07-15T00:00:00Z"
```

After creating milestones, fetch their IDs:
```
gh api repos/{owner}/{repo}/milestones --jq '.[] | "\(.number) \(.title)"'
```

Store the milestone numbers for issue assignment below.

---

## Phase 3: Create Issues

Read each RFC carefully. For each RFC, decompose into focused, implementable issues. Every issue must have:
- A clear title: `[<scope>] <imperative description>`
- A body with: **Context** (which RFC, which section), **Acceptance Criteria** (checkboxes), **Dependencies** (which issues must be done first), **Branch** (gitflow branch name)
- Labels: scope, type, priority, rfc reference
- Milestone assignment

### M0: Project Scaffold

Read RFC-0002 (Technology Stack) and RFC-0011 (DNA & Provenance).

Create these issues:

**Issue: `[common] Scaffold Cargo workspace with all crate stubs`**
- Body: Set up the root `Cargo.toml` workspace with all 12 crates as members. Each crate gets a minimal `Cargo.toml`, `src/lib.rs` with license header, and `src/error.rs` with `thiserror` enum. No application logic — just compilable stubs. Ref: RFC-0002 §3.
- Acceptance Criteria:
  - [ ] Root Cargo.toml lists all workspace members
  - [ ] Each crate has Cargo.toml with correct metadata (name, version, edition, license, repository)
  - [ ] Each crate has src/lib.rs with BSL-1.1 license header
  - [ ] Each crate has src/error.rs with thiserror Error enum
  - [ ] `cargo check --workspace` passes
  - [ ] `cargo clippy --workspace -- -D warnings` passes
- Labels: `scope:common`, `type:chore`, `priority:critical`, `rfc:0002`
- Milestone: M0
- Dependencies: None (this is the first issue)
- Branch: `feature/GH-{number}-scaffold-workspace`

**Issue: `[common] Define shared types — RunId, EdgeId, ScenarioId, GasRate, newtypes`**
- Body: Create newtype wrappers for all domain identifiers and units. Include Display, FromStr, Serialize, Deserialize. Ref: RFC-0002 §3, RFC-0003 §4.
- Acceptance Criteria:
  - [ ] RunId, EdgeId, ScenarioId, PhaseId as Uuid newtypes
  - [ ] GasRate, TxCount as u64 newtypes
  - [ ] All implement Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Display, FromStr
  - [ ] Unit tests for serialization round-trips (proptest)
  - [ ] Re-exported from gashammer-common
- Labels: `scope:common`, `type:feature`, `priority:critical`, `rfc:0002`
- Milestone: M0

**Issue: `[common] Implement build_info.rs with DNA canary and version metadata`**
- Body: Embed compile-time provenance: version, git SHA, build time, copyright, canary string. Wire up build.rs to inject env vars. Ref: RFC-0011 §7.
- Acceptance Criteria:
  - [ ] GASHAMMER_CANARY static string embedded in binary
  - [ ] Version, build SHA, build time available at runtime
  - [ ] `strings` on compiled binary reveals canary
  - [ ] build.rs sets GASHAMMER_BUILD_SHA and GASHAMMER_BUILD_TIME
- Labels: `scope:common`, `type:feature`, `priority:critical`, `rfc:0011`
- Milestone: M0

**Issue: `[common] Define GH-Exxx error code registry`**
- Body: Create a central error code enum covering all crates. Each variant has a unique GH-Exxx code, Display impl, and human-readable message. Ref: RFC-0011 §8.3.
- Acceptance Criteria:
  - [ ] Error codes follow GH-Exxx scheme (GH-E001 through GH-E999)
  - [ ] Ranges allocated per crate (E001-E099 common, E100-E199 nitro, etc.)
  - [ ] Display impl includes the code: "GH-E001: Scenario validation failed"
  - [ ] Documented in a table
- Labels: `scope:common`, `type:feature`, `priority:high`, `rfc:0011`
- Milestone: M0

**Issue: `[ci] Set up GitHub Actions — build, test, clippy, fmt, license check`**
- Body: Create .github/workflows/ci.yml with the full verification pipeline. Runs on every PR to develop. Ref: RFC-0002 §9.
- Acceptance Criteria:
  - [ ] cargo fmt --check
  - [ ] cargo clippy --workspace --all-targets -- -D warnings
  - [ ] cargo test --workspace
  - [ ] cargo deny check licenses
  - [ ] Runs on: push to develop, PRs to develop and main
  - [ ] Caches cargo registry and build artifacts
- Labels: `scope:ci`, `type:chore`, `priority:critical`, `rfc:0002`
- Milestone: M0

**Issue: `[ci] Add license header enforcement to CI`**
- Body: Ensure every .rs file contains the BSL-1.1 header. Fail CI if any file is missing it. Ref: RFC-0011 §3.3.
- Acceptance Criteria:
  - [ ] Script or tool checks all .rs files for license header
  - [ ] CI step fails if any file is missing
  - [ ] Clear error message listing offending files
- Labels: `scope:ci`, `type:chore`, `priority:high`, `rfc:0011`
- Milestone: M0

**Issue: `[proto] Define gRPC protobuf schemas for hive-edge communication`**
- Body: Create proto/gashammer/v1/*.proto files defining the HiveEdge service, registration, run control, telemetry streaming, and barrier sync. Ref: RFC-0003 §6.
- Acceptance Criteria:
  - [ ] proto/gashammer/v1/hive_edge.proto with all service methods
  - [ ] proto/gashammer/v1/types.proto with shared message types
  - [ ] tonic-build integration in build.rs
  - [ ] Generated Rust code compiles
  - [ ] Package name: gashammer.v1 (DNA provenance)
- Labels: `scope:proto`, `type:feature`, `priority:high`, `rfc:0003`
- Milestone: M0

**Issue: `[infra] Create Dockerfile with OCI labels and DNA provenance`**
- Body: Multi-stage Dockerfile for hive and edge binaries. Include all OCI labels per RFC-0011 §6.4. Ref: RFC-0002 §10, RFC-0011 §6.4.
- Acceptance Criteria:
  - [ ] Multi-stage build (builder + runtime)
  - [ ] Runtime image based on distroless or alpine
  - [ ] All OCI labels present (title, description, authors, url, source, licenses, build SHA, version)
  - [ ] `docker inspect` shows all labels
  - [ ] Binary includes canary string
- Labels: `scope:infra`, `type:chore`, `priority:medium`, `rfc:0011`
- Milestone: M0

### M1: Wire Protocol & Nitro Adapters

Read RFC-0001 (Nitro Integration Surface).

**Issue: `[nitro] Implement RPC provider adapter — sequencer + gateway JSON-RPC`**
- Body: Wrap alloy Provider for Nitro-specific RPC calls. Support sequencer (send_tx, pending state) and gateway (confirmed state, historical). Connection pooling, retry with backoff, health checks. Ref: RFC-0001 §2.
- Acceptance Criteria:
  - [ ] NitroRpcProvider trait with sequencer and gateway variants
  - [ ] eth_sendRawTransaction, eth_getTransactionReceipt, eth_getBalance, eth_call
  - [ ] Connection health check (periodic eth_chainId)
  - [ ] Retry with exponential backoff on transient errors
  - [ ] Configurable timeout and max retries
  - [ ] Unit tests with mock RPC responses
- Labels: `scope:nitro`, `type:feature`, `priority:critical`, `rfc:0001`
- Milestone: M1

**Issue: `[nitro] Implement Nitro feed consumer — WebSocket subscription and message parsing`**
- Body: Connect to Nitro sequencer feed relay via WebSocket. Parse BroadcastFeedMessage, extract transactions and L2 block data. Handle reconnection. Ref: RFC-0001 §3.
- Acceptance Criteria:
  - [ ] WebSocket connection to feed relay
  - [ ] Parse BroadcastFeedMessage JSON frames
  - [ ] Extract transaction hashes and sequence numbers
  - [ ] Automatic reconnection with backoff on disconnect
  - [ ] Emit parsed events via tokio::sync::mpsc channel
  - [ ] Metrics: messages received, parse errors, reconnections
- Labels: `scope:nitro`, `type:feature`, `priority:critical`, `rfc:0001`
- Milestone: M1

**Issue: `[nitro] Implement L1 contract reader — SequencerInbox and RollupCore`**
- Body: Read Nitro L1 contracts for batch posting data and rollup state. Used by telemetry and oracle. Ref: RFC-0001 §4.
- Acceptance Criteria:
  - [ ] Read SequencerInbox.batchCount() and batch data
  - [ ] Read RollupCore.latestConfirmed() and assertion data
  - [ ] ABI types generated from Nitro contract ABIs
  - [ ] Configurable L1 RPC endpoint
  - [ ] Polling interval configurable
- Labels: `scope:nitro`, `type:feature`, `priority:high`, `rfc:0001`
- Milestone: M1

**Issue: `[nitro] Add User-Agent and HTTP header DNA to all outbound requests`**
- Body: Ensure every HTTP and WebSocket request includes GasHammer User-Agent and custom headers. Ref: RFC-0011 §4.2-4.3.
- Acceptance Criteria:
  - [ ] User-Agent: GasHammer/<version> (https://github.com/copyleftdev/gashammer; <sha>)
  - [ ] Applied to RPC provider, feed consumer, and any HTTP client
  - [ ] Headers visible in access logs of target nodes
- Labels: `scope:nitro`, `type:feature`, `priority:medium`, `rfc:0011`
- Milestone: M1

**Issue: `[testenv] Scaffold testcontainers devnet — L1 Geth + Sequencer + Feed Relay`**
- Body: Create gashammer-testenv crate with container definitions for Nitro devnet minimal profile. Ref: RFC-0013 §3-4.
- Acceptance Criteria:
  - [ ] L1Geth container (dev mode, instant finality)
  - [ ] NitroSequencer container (sequencer + feed output)
  - [ ] FeedRelay container
  - [ ] NitroDevnet::minimal() boots all three in <30s
  - [ ] Health checks for each component
  - [ ] Account pre-funding (100 accounts × 100 ETH)
  - [ ] Smoke test passes: `cargo test -p gashammer-testenv`
- Labels: `scope:testenv`, `type:feature`, `priority:high`, `rfc:0013`
- Milestone: M1

**Issue: `[nitro] Integration test — submit tx to devnet sequencer and get receipt`**
- Body: First real integration test. Uses testcontainers devnet. Submit a tx, get receipt, verify success. Ref: RFC-0013 §7.2.
- Acceptance Criteria:
  - [ ] Test boots NitroDevnet::minimal()
  - [ ] Submits a simple ETH transfer via NitroRpcProvider
  - [ ] Gets receipt, asserts success status and gas used
  - [ ] Test passes in CI with Docker
- Labels: `scope:nitro`, `scope:testenv`, `type:test`, `priority:high`, `rfc:0013`
- Milestone: M1

### M2: Core Engine

Read RFC-0003 (Hive), RFC-0004 (Edge), RFC-0005 (Workload), RFC-0006 (Telemetry).

**Issue: `[hive] Implement edge registry — registration, heartbeat, deregistration`**
- Body: Hive maintains a registry of connected edges. gRPC RegisterEdge, Heartbeat, DeregisterEdge. Ref: RFC-0003 §4-5.
- Acceptance Criteria:
  - [ ] EdgeRegistry struct with concurrent-safe edge map
  - [ ] RegisterEdge gRPC handler — assigns edge ID, records capabilities
  - [ ] Heartbeat handler — updates last_seen, detects stale edges
  - [ ] DeregisterEdge handler — removes from registry
  - [ ] Stale edge reaper task (configurable timeout)
  - [ ] Metrics: edges_registered, edges_active, heartbeat_latency
- Labels: `scope:hive`, `type:feature`, `priority:critical`, `rfc:0003`
- Milestone: M2

**Issue: `[hive] Implement REST API — scenario CRUD, run lifecycle, edge status`**
- Body: axum REST API for human/CI interaction. Ref: RFC-0003 §7.
- Acceptance Criteria:
  - [ ] POST /scenarios — upload and validate scenario YAML
  - [ ] GET /scenarios — list scenarios
  - [ ] POST /runs — start a run from a scenario
  - [ ] GET /runs/{id} — run status
  - [ ] GET /runs/{id}/report — download report
  - [ ] GET /edges — list connected edges
  - [ ] X-Powered-By: GasHammer/<version> header on all responses
  - [ ] OpenAPI spec generated or hand-maintained
- Labels: `scope:hive`, `type:feature`, `priority:critical`, `rfc:0003`
- Milestone: M2

**Issue: `[hive] Implement run orchestration — phase progression, barrier sync, completion`**
- Body: Hive coordinates a run across edges: start, phase transitions, barrier synchronization, stop, and result collection. Ref: RFC-0003 §5-6.
- Acceptance Criteria:
  - [ ] Run state machine: Created → Starting → Running → Stopping → Completed/Failed
  - [ ] Phase progression with barrier sync across edges
  - [ ] Timeout handling for unresponsive edges
  - [ ] Graceful stop on error or user cancellation
  - [ ] Run result aggregation from all edges
- Labels: `scope:hive`, `type:feature`, `priority:critical`, `rfc:0003`
- Milestone: M2

**Issue: `[edge] Implement edge runtime — startup, registration, connection management`**
- Body: Edge binary startup sequence: parse config, connect to sequencer/feed/hive, register with hive. Ref: RFC-0004 §2-4.
- Acceptance Criteria:
  - [ ] CLI binary with clap argument parsing
  - [ ] Config file loading (TOML)
  - [ ] Connect to sequencer RPC, gateway RPC, feed relay
  - [ ] Register with hive via gRPC
  - [ ] Heartbeat task
  - [ ] Graceful shutdown on SIGTERM/SIGINT
  - [ ] Startup log line with version and copyright (DNA)
- Labels: `scope:edge`, `type:feature`, `priority:critical`, `rfc:0004`
- Milestone: M2

**Issue: `[edge] Implement transaction pipeline — build, sign, submit, track`**
- Body: Core tx pipeline: receive template from workload engine, build tx, sign, submit to sequencer, track receipt. Ref: RFC-0004 §5.
- Acceptance Criteria:
  - [ ] TxPipeline struct with configurable concurrency
  - [ ] Build tx from template + account pool
  - [ ] Sign with account private key
  - [ ] Submit via NitroRpcProvider
  - [ ] Track pending txs, poll for receipts
  - [ ] Emit TxSubmitted and TxConfirmed events to telemetry
  - [ ] Metrics: tx_submitted, tx_confirmed, tx_failed, tx_latency
- Labels: `scope:edge`, `type:feature`, `priority:critical`, `rfc:0004`
- Milestone: M2

**Issue: `[edge] Implement feed correlator — match submitted txs to feed messages`**
- Body: Correlate submitted transactions with their appearance in the sequencer feed. Measures sequencer inclusion latency. Ref: RFC-0004 §6.
- Acceptance Criteria:
  - [ ] Maintain pending tx set (hash → submit timestamp)
  - [ ] Match feed messages against pending set
  - [ ] Emit FeedCorrelation events with inclusion latency
  - [ ] Handle timeouts for txs that never appear in feed
  - [ ] Metrics: correlation_latency, correlation_misses
- Labels: `scope:edge`, `type:feature`, `priority:high`, `rfc:0004`
- Milestone: M2

**Issue: `[workload] Implement transaction templates — transfer, ERC-20, storage, compute`**
- Body: Define tx templates with gas profiles. Each template knows how to build a concrete transaction. Ref: RFC-0005 §2-3.
- Acceptance Criteria:
  - [ ] Template trait with build_tx method
  - [ ] SimpleTransfer template
  - [ ] Erc20Transfer template
  - [ ] StorageWrite template (configurable slot count)
  - [ ] ComputeHeavy template (configurable gas burn)
  - [ ] Each template has an estimated gas profile
  - [ ] Calldata magic bytes appended where applicable (DNA)
- Labels: `scope:workload`, `type:feature`, `priority:critical`, `rfc:0005`
- Milestone: M2

**Issue: `[workload] Implement workload engine — rate control, gas targeting, temporal patterns`**
- Body: Engine that drives tx generation at a target gas/sec rate using weighted template selection. Ref: RFC-0005 §4-5.
- Acceptance Criteria:
  - [ ] Target gas rate (sustained, burst, ramp)
  - [ ] Weighted random template selection
  - [ ] Rate limiter (token bucket or leaky bucket)
  - [ ] Deterministic PRNG from seed
  - [ ] Same seed + same config = same tx sequence
  - [ ] Metrics: gas_submitted_per_sec, tx_rate, template_distribution
- Labels: `scope:workload`, `type:feature`, `priority:critical`, `rfc:0005`
- Milestone: M2

**Issue: `[workload] Implement account pool — HD derivation, partitioning, nonce tracking`**
- Body: Manage test accounts with deterministic derivation, per-edge partitioning, and local nonce tracking. Ref: RFC-0005 §6.
- Acceptance Criteria:
  - [ ] HD derivation at m/44'/60'/0'/0'/0x4748/{index} (DNA)
  - [ ] Partition accounts across edges (no overlap)
  - [ ] Local nonce tracking per account
  - [ ] Nonce recovery on tx failure
  - [ ] Account rotation to avoid hot-account bottleneck
- Labels: `scope:workload`, `type:feature`, `priority:high`, `rfc:0005`
- Milestone: M2

**Issue: `[telemetry] Implement event model and telemetry pipeline`**
- Body: Define the event taxonomy, in-memory ring buffer, and shipper that sends events from edge to hive. Ref: RFC-0006 §2-4.
- Acceptance Criteria:
  - [ ] Event enum covering all event types (TxSubmitted, TxConfirmed, FeedMessage, OracleVerdict, etc.)
  - [ ] Lock-free ring buffer for edge-local event storage
  - [ ] Telemetry shipper: batch events and send to hive via gRPC stream
  - [ ] Backpressure: drop oldest events when buffer full (with counter)
  - [ ] Metrics: events_generated, events_shipped, events_dropped
- Labels: `scope:telemetry`, `type:feature`, `priority:critical`, `rfc:0006`
- Milestone: M2

**Issue: `[telemetry] Implement Parquet storage for telemetry events`**
- Body: Hive-side storage of telemetry events in Parquet format with GasHammer metadata in file footer. Ref: RFC-0006 §5, RFC-0011 §6.2.
- Acceptance Criteria:
  - [ ] Write events to Parquet files (partitioned by run_id and time)
  - [ ] Custom Parquet metadata: gashammer.version, gashammer.build, gashammer.run_id, gashammer.copyright (DNA)
  - [ ] Configurable file rotation (by size or time)
  - [ ] Read-back API for report generation
- Labels: `scope:telemetry`, `type:feature`, `priority:high`, `rfc:0006`
- Milestone: M2

### M3: Verification & Fault Injection

Read RFC-0007 (Correctness Oracle) and RFC-0008 (Fault Injection).

**Issue: `[oracle] Implement correctness oracle — invariant framework and live checks`**
- Body: Define invariant trait and implement live checks: balance conservation, nonce monotonicity, gas accounting. Ref: RFC-0007 §2-3.
- Acceptance Criteria:
  - [ ] Invariant trait with check() method returning Verdict
  - [ ] BalanceConservation invariant
  - [ ] NonceMonotonicity invariant
  - [ ] GasAccounting invariant (gas_used ≤ gas_limit)
  - [ ] Verdict enum: Pass, Fail, Inconclusive with evidence
  - [ ] Live check runner that evaluates invariants during run
  - [ ] Metrics: oracle_checks_total, oracle_violations_total
- Labels: `scope:oracle`, `type:feature`, `priority:critical`, `rfc:0007`
- Milestone: M3

**Issue: `[oracle] Implement post-run checks — state diff, ordering, replay equivalence`**
- Body: Checks that run after workload completes. State diff analysis, transaction ordering verification. Ref: RFC-0007 §4.
- Acceptance Criteria:
  - [ ] State diff: compare pre-run and post-run state roots
  - [ ] Ordering: verify tx inclusion order matches submission order (where guaranteed)
  - [ ] Duplicate detection: no tx included twice
  - [ ] Verdict aggregation across all checks
  - [ ] Evidence collection for failed checks
- Labels: `scope:oracle`, `type:feature`, `priority:high`, `rfc:0007`
- Milestone: M3

**Issue: `[fault] Implement fault injection adapter trait and network fault adapter`**
- Body: Define the FaultAdapter trait and implement network faults (latency, packet loss, partition) via netem/iptables. Ref: RFC-0008 §2-3.
- Acceptance Criteria:
  - [ ] FaultAdapter trait: inject(), revert(), describe()
  - [ ] NetworkFaultAdapter using tc netem (latency, loss, corruption)
  - [ ] PartitionAdapter using iptables (block traffic between components)
  - [ ] Fault journal: log every injection and reversion
  - [ ] Safety: automatic revert on timeout or panic
  - [ ] Metrics: faults_injected, faults_reverted, fault_duration
- Labels: `scope:fault`, `type:feature`, `priority:high`, `rfc:0008`
- Milestone: M3

**Issue: `[fault] Implement fault timeline and scheduling`**
- Body: Schedule faults at specific times or phases within a run. Support overlapping faults. Ref: RFC-0008 §4.
- Acceptance Criteria:
  - [ ] FaultTimeline: ordered list of (time, fault) pairs
  - [ ] Parsed from scenario SDL fault_schedule section
  - [ ] Executor applies faults at scheduled times
  - [ ] Overlapping faults supported
  - [ ] All faults reverted at run end (safety guarantee)
- Labels: `scope:fault`, `type:feature`, `priority:medium`, `rfc:0008`
- Milestone: M3

### M4: Scenarios & Reporting

Read RFC-0009 (Reporting Engine) and RFC-0010 (Scenario Definition Language).

**Issue: `[scenario] Implement SDL parser — YAML schema, validation, compilation`**
- Body: Parse scenario YAML files into compiled scenario structs. JSON Schema validation. Ref: RFC-0010 §2-4.
- Acceptance Criteria:
  - [ ] Serde-based YAML deserialization into ScenarioDefinition
  - [ ] JSON Schema for IDE autocompletion and pre-validation
  - [ ] Validation: required fields, value ranges, template references
  - [ ] Compilation: resolve references, compute derived values, produce CompiledScenario
  - [ ] Clear error messages with line numbers on validation failure
  - [ ] Compilation signature embedded (DNA)
- Labels: `scope:scenario`, `type:feature`, `priority:critical`, `rfc:0010`
- Milestone: M4

**Issue: `[scenario] Create example scenarios for common test patterns`**
- Body: Ship ready-to-use scenario files demonstrating various test patterns. Ref: RFC-0010 §7.
- Acceptance Criteria:
  - [ ] simple-transfer-load.yaml — basic throughput test
  - [ ] mixed-workload.yaml — multiple templates with weights
  - [ ] ramp-to-saturation.yaml — find the breaking point
  - [ ] fault-tolerance.yaml — inject network faults during load
  - [ ] correctness-audit.yaml — low rate with full oracle checks
  - [ ] All scenarios validate against JSON Schema
- Labels: `scope:scenario`, `type:feature`, `priority:medium`, `rfc:0010`
- Milestone: M4

**Issue: `[report] Implement standard run report — summary, latency breakdown, timeline`**
- Body: Generate a structured report from run telemetry. JSON and Markdown formats. Ref: RFC-0009 §2.
- Acceptance Criteria:
  - [ ] RunReport struct with summary, phases, latency percentiles, throughput curves
  - [ ] JSON serialization with provenance block (DNA)
  - [ ] Markdown rendering with tables and sections
  - [ ] Report footer with GasHammer attribution (DNA)
  - [ ] Latency percentiles: p50, p90, p95, p99, p99.9
  - [ ] Throughput: gas/sec and tx/sec time series
- Labels: `scope:report`, `type:feature`, `priority:critical`, `rfc:0009`
- Milestone: M4

**Issue: `[report] Implement capacity envelope and regression detection`**
- Body: Compute safe operating envelope from run data. Detect regressions against baseline. Ref: RFC-0009 §3-4.
- Acceptance Criteria:
  - [ ] CapacityEnvelope: max sustainable gas rate at p99 < threshold
  - [ ] Regression detection: compare current run to baseline
  - [ ] Statistical significance test (not just "number is different")
  - [ ] Release gate verdict: pass/fail/warn
  - [ ] Bottleneck classification: CPU/memory/network/sequencer
- Labels: `scope:report`, `type:feature`, `priority:high`, `rfc:0009`
- Milestone: M4

### M5: E2E Testing & DevEx

Read RFC-0012 (Documentation Engine) and RFC-0013 (E2E Testing).

**Issue: `[testenv] Deploy test contracts — Counter, ERC-20, GasBurner, Reverter, EventEmitter`**
- Body: Compile and embed Solidity test contracts. Deploy during devnet setup. Ref: RFC-0013 §10.
- Acceptance Criteria:
  - [ ] Solidity source in contracts/test/
  - [ ] Compiled bytecode embedded as const byte arrays
  - [ ] TestContracts struct with deployed addresses
  - [ ] NitroDevnet::deploy_test_contracts() method
  - [ ] No Solidity toolchain needed at test time
- Labels: `scope:testenv`, `type:feature`, `priority:medium`, `rfc:0013`
- Milestone: M5

**Issue: `[testenv] Full lifecycle integration test — hive + edges + run + report`**
- Body: End-to-end test: boot devnet, start hive, connect edge, run scenario, collect report. Ref: RFC-0013 §6.
- Acceptance Criteria:
  - [ ] Boot NitroDevnet::standard()
  - [ ] Start hive process (in-process or container)
  - [ ] Start edge process connecting to devnet and hive
  - [ ] Submit scenario via hive REST API
  - [ ] Run completes successfully
  - [ ] Report generated with valid provenance
  - [ ] Oracle checks pass
- Labels: `scope:testenv`, `scope:hive`, `scope:edge`, `type:test`, `priority:high`, `rfc:0013`
- Milestone: M5

**Issue: `[docgen] Implement xtask docgen — parse source with syn, generate mdBook pages`**
- Body: cargo xtask docgen parses Rust source files, extracts public API and doc comments, generates mdBook pages. Ref: RFC-0012 §3-5.
- Acceptance Criteria:
  - [ ] Parse all crates with syn
  - [ ] Extract public items: functions, structs, enums, traits, impls
  - [ ] Extract doc comments
  - [ ] Generate per-module mdBook pages via tera templates
  - [ ] Generate SUMMARY.md entries (preserve hand-written sections)
  - [ ] Coverage metric: % of public items with doc comments
  - [ ] CI fails if coverage drops below threshold
- Labels: `scope:docgen`, `type:feature`, `priority:medium`, `rfc:0012`
- Milestone: M5

**Issue: `[docs] Build mdBook documentation site with architecture guides`**
- Body: Write hand-authored documentation for getting started, architecture overview, and operational guides. Ref: RFC-0012.
- Acceptance Criteria:
  - [ ] Getting started guide (install, configure, first run)
  - [ ] Architecture overview (hive/edge topology, data flow)
  - [ ] Scenario authoring guide
  - [ ] Operational guide (deployment, monitoring, troubleshooting)
  - [ ] mdbook build produces clean output
  - [ ] Deployed to GitHub Pages
- Labels: `scope:docs`, `type:feature`, `priority:medium`, `rfc:0012`
- Milestone: M5

### M6: Release Candidate

**Issue: `[ci] Add signed release pipeline — Ed25519 signatures, SBOM, checksums`**
- Body: GitHub Actions release workflow that builds, signs, and publishes binaries. Ref: RFC-0011 §9.
- Acceptance Criteria:
  - [ ] Build Linux amd64 and arm64 binaries
  - [ ] SHA-256 checksums for all artifacts
  - [ ] Ed25519 signatures for all artifacts
  - [ ] CycloneDX SBOM generation
  - [ ] GitHub Release with all artifacts attached
  - [ ] Container images pushed to GHCR with OCI labels
- Labels: `scope:ci`, `type:chore`, `priority:high`, `rfc:0011`
- Milestone: M6

**Issue: `[infra] Create Helm chart with provenance annotations`**
- Body: Helm chart for deploying hive and edge to Kubernetes. Ref: RFC-0002 §10, RFC-0011 §6.5.
- Acceptance Criteria:
  - [ ] Chart.yaml with DNA annotations (author, license, source)
  - [ ] Hive deployment + service
  - [ ] Edge daemonset or deployment (configurable replicas)
  - [ ] ConfigMap for hive and edge TOML config
  - [ ] ServiceMonitor for Prometheus scraping
  - [ ] values.yaml with sensible defaults
- Labels: `scope:infra`, `type:feature`, `priority:medium`, `rfc:0002`
- Milestone: M6

---

## Phase 4: Create Each Issue

For each issue defined above, run:

```
gh issue create \
  --title "<title>" \
  --body "<body with acceptance criteria>" \
  --label "<label1>,<label2>,..." \
  --milestone "<milestone title>"
```

After creating all issues, verify:

// turbo
```
gh issue list --state open --limit 100 --json number,title,milestone,labels --jq '.[] | "\(.number) [\(.milestone.title)] \(.title)"'
```

---

## Phase 5: Summary Report

After all issues are created, produce a summary:

**BACKLOG BOOTSTRAP COMPLETE**

| Category | Count |
|----------|-------|
| Labels created | N |
| Milestones created | 7 |
| Issues created | N |

**Milestone Breakdown:**
- M0: Project Scaffold — N issues
- M1: Wire Protocol — N issues
- M2: Core Engine — N issues
- M3: Verification — N issues
- M4: Scenarios & Reporting — N issues
- M5: E2E & DevEx — N issues
- M6: Release Candidate — N issues

**First issue to work on:**
Issue #1: `[common] Scaffold Cargo workspace with all crate stubs`
Branch: `feature/GH-1-scaffold-workspace`
Command: `git checkout develop && git checkout -b feature/GH-1-scaffold-workspace`

**IMPORTANT:** After creating all issues, verify that `develop` branch exists. If not, create it from `main`:
```
git checkout main && git checkout -b develop && git push -u origin develop
```
