# CLAUDE.md — GasHammer

## What is this project?

GasHammer is an adversarial load-testing and correctness-verification platform for Arbitrum Nitro rollup infrastructure. Written in Rust. Targets Offchain Labs' stack via wire protocols only (JSON-RPC, WebSocket feed, gRPC). No internal linking against Nitro.

Owner: Don Johnson (@copyleftdev). Licensed BSL-1.1.

## Build / Test / Lint commands

```bash
# Build everything
cargo build --workspace

# Build a specific crate
cargo build -p gashammer-edge

# Run all unit tests
cargo test --workspace

# Run a specific crate's tests
cargo test -p gashammer-nitro

# Run integration tests (requires Docker)
cargo test --test integration -- --test-threads=1

# Run E2E tests (requires Docker, slower)
GASHAMMER_TEST_PROFILE=standard cargo test --test e2e -- --test-threads=1

# Run a single test by name
cargo test --workspace -- test_name_substring

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Format check
cargo fmt --all -- --check

# Format fix
cargo fmt --all

# Type check only (fast)
cargo check --workspace

# Generate docs
cargo xtask docgen

# Build mdBook
mdbook build docs/

# License header check
cargo deny check licenses
```

## Code style

### Rust conventions

- **Edition:** 2021. **MSRV:** 1.75.0.
- **Error handling:** Use `thiserror` for library errors, `anyhow` only in binaries and tests. Never `unwrap()` in library code. Never `expect()` without a message explaining the invariant.
- **Async runtime:** `tokio` only. No `async-std`. No mixing runtimes.
- **Imports:** Group in this order, separated by blank lines: (1) `std`, (2) external crates, (3) workspace crates, (4) `crate`/`super`/`self`.
- **Naming:** Crates are `gashammer-{name}` in Cargo.toml, `gashammer_{name}` as Rust modules. No abbreviations in public APIs except: `tx` (transaction), `rpc` (remote procedure call), `ws` (WebSocket), `config` (configuration).
- **Visibility:** Default to `pub(crate)`. Only `pub` items that are part of the crate's API contract. No `pub` fields on structs unless there's a documented reason.
- **Derive order:** `#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]` — always in this canonical order, omitting what's not needed.
- **Constants:** Use `const` not `static` unless interior mutability is required. Put constants at the top of the file after imports.
- **String formatting:** Prefer `format!()` over string concatenation. Use `tracing` structured fields, not string interpolation in log messages.

### Patterns to follow

- **Builder pattern** for any struct with >3 fields or optional fields.
- **Newtype wrappers** for domain types: `RunId(Uuid)`, `EdgeId(Uuid)`, `GasRate(u64)`.
- **`From`/`Into` impls** over manual conversion methods.
- **Trait objects** (`dyn Trait`) for plugin points (fault adapters, invariant checks). Generics for hot paths.
- **Channel-based coordination** between async tasks. No shared mutable state via `Arc<Mutex<_>>` unless unavoidable. Prefer `tokio::sync::mpsc`, `broadcast`, `watch`.
- **Structured logging** via `tracing`. Every function that can fail should have a `tracing::instrument` attribute or manual span.

### Anti-patterns — DO NOT

- Do not use `println!` or `eprintln!`. Use `tracing::info!`, `tracing::error!`.
- Do not use `Box<dyn Error>` — use concrete error types with `thiserror`.
- Do not use `tokio::spawn` without storing the `JoinHandle`. Detached tasks are bugs.
- Do not use `sleep` for synchronization. Use channels, barriers, or `Notify`.
- Do not add dependencies without checking if an existing dep covers the need.
- Do not write `impl` blocks in separate files from the type definition unless the file exceeds 500 lines.
- Do not use `#[allow(dead_code)]` — delete dead code instead.
- Do not use string literals for identifiers that should be enums or constants.
- Do not use `clone()` to satisfy the borrow checker without a comment explaining why.

## Project structure

```
gashammer/
├── CLAUDE.md                 # ← you are here
├── Cargo.toml                # workspace root
├── crates/
│   ├── gashammer-common/     # shared types, config, build_info, error codes
│   ├── gashammer-nitro/      # Nitro protocol adapters (RPC, feed, contracts)
│   ├── gashammer-edge/       # edge runtime binary
│   ├── gashammer-hive/       # hive control plane binary
│   ├── gashammer-workload/   # workload engine, tx templates, gas modeling
│   ├── gashammer-telemetry/  # event model, pipeline, Parquet storage
│   ├── gashammer-oracle/     # correctness checks, invariants, verdicts
│   ├── gashammer-fault/      # fault injection adapters, timeline, safety
│   ├── gashammer-report/     # report generation, regression, capacity envelope
│   ├── gashammer-scenario/   # SDL parser, validator, compiler
│   ├── gashammer-docgen/     # mdBook generator from source code
│   └── gashammer-testenv/    # testcontainers Nitro devnet orchestrator
├── xtask/                    # cargo xtask commands (docgen, build-contracts)
├── proto/                    # protobuf definitions (hive-edge gRPC)
├── contracts/test/           # Solidity test contracts
├── docs/                     # mdBook documentation
│   ├── book.toml
│   └── src/
├── rfcs/                     # 13 RFCs — the spec corpus
├── tests/
│   ├── integration/          # testcontainers integration tests
│   └── e2e/                  # full lifecycle E2E tests
└── .claude/                  # Claude Code project settings
```

## Architecture (decision log, not docs)

1. **Wire-protocol-only integration.** GasHammer never imports Nitro crate code. All interaction is via JSON-RPC (alloy), WebSocket (tokio-tungstenite), gRPC (tonic), and L1 contract reads (alloy). See RFC-0001.
2. **Hive/Edge topology.** Hive is the control plane (one instance). Edges are regional workers (many instances). Communication is gRPC with mTLS. See RFC-0003, RFC-0004.
3. **Gas-first workload modeling.** Workloads are defined by target gas/sec, not requests/sec. Templates specify gas profiles. See RFC-0005.
4. **Deterministic PRNG.** All workload generation uses seeded PRNGs for reproducibility. Same seed + same scenario = same transactions. See RFC-0005 §1.
5. **Testcontainers for E2E.** Real Nitro devnet in Docker for every integration test. No mocks for protocol-level tests. See RFC-0013.
6. **BSL-1.1 license.** Same as Nitro itself. Per-file headers enforced in CI. See RFC-0011.

## Key domain terminology

- **Hive** — control plane process
- **Edge** — regional worker process
- **Run** — a single execution of a scenario
- **Phase** — a time segment within a run (each phase can have different workload config)
- **Safe Operating Envelope** — measured capacity range of the system under test
- **Verdict** — pass/fail result from the correctness oracle
- **SDL** — Scenario Definition Language (YAML-based)

## RFCs

Specs live in `rfcs/`. There are 13 RFCs (RFC-0001 through RFC-0013). Always check the relevant RFC before implementing a feature. If the implementation diverges from the RFC, update the RFC first.

| RFC | Topic |
|-----|-------|
| 0001 | Nitro Integration Surface |
| 0002 | Technology Stack |
| 0003 | Hive Control Plane |
| 0004 | Edge Runtime |
| 0005 | Workload Engine |
| 0006 | Telemetry Pipeline |
| 0007 | Correctness Oracle |
| 0008 | Fault Injection |
| 0009 | Reporting Engine |
| 0010 | Scenario Definition Language |
| 0011 | DNA & Provenance |
| 0012 | Documentation Engine |
| 0013 | E2E Testing (Testcontainers + Nitro Devnet) |

## Testing rules

1. Every new public function gets a test. No exceptions.
2. Test file lives next to the source: `mod tests` block at bottom of file for unit tests, `tests/` directory for integration.
3. Use `#[tokio::test]` for async tests. Never `block_on`.
4. Test names describe the behavior: `test_feed_reconnects_after_disconnect`, not `test_feed_1`.
5. Integration tests that need Docker are gated by `#[cfg(feature = "integration")]` or live in `tests/integration/`.
6. Never test private implementation details. Test the public contract.
7. Use `proptest` for property-based tests on serialization round-trips and workload generation.
8. Correctness oracle checks must have both passing and failing test cases.

## Gitflow (STRICT — NO EXCEPTIONS)

This project uses **strict gitflow**. Every rule below is non-negotiable. Violations are treated as bugs.

### Branch model

```text
main ──────────────────────────────────────────── production-ready, tagged releases only
  │
  └── develop ─────────────────────────────────── integration branch, all features merge here
        │
        ├── feature/GH-<issue#>-<description> ── new work, branches from develop
        ├── fix/GH-<issue#>-<description> ────── bug fixes, branches from develop
        ├── release/<version> ────────────────── release prep, branches from develop, merges to main AND develop
        └── hotfix/GH-<issue#>-<description> ── emergency fixes, branches from main, merges to main AND develop
```

### Branch rules

1. **`main` is sacred.** Never commit directly to `main`. Ever. Not even "just this once."
2. **`develop` is the integration target.** All feature and fix branches merge to `develop` via PR.
3. **Feature branches** branch from `develop`, merge back to `develop`. Named `feature/GH-<issue#>-<description>`.
4. **Fix branches** branch from `develop`, merge back to `develop`. Named `fix/GH-<issue#>-<description>`.
5. **Release branches** branch from `develop` when ready to release. Named `release/<version>` (e.g., `release/0.1.0`). Only bug fixes and version bumps allowed. Merges to **both** `main` (with tag) and `develop`.
6. **Hotfix branches** branch from `main` for critical production fixes. Named `hotfix/GH-<issue#>-<description>`. Merges to **both** `main` (with tag) and `develop`.
7. **Delete branches after merge.** No stale branches. No exceptions.

### GitHub Issues (MANDATORY)

**No work happens without a GitHub Issue. Period.**

- Every feature, bug fix, refactor, RFC implementation, doc change, CI change — **must** have an issue first.
- Issue title format: `[<scope>] <imperative description>` — e.g., `[edge] Add feed reconnection with exponential backoff`
- Issues must have: description, acceptance criteria, and labels (`feature`, `bug`, `refactor`, `rfc`, `docs`, `ci`, `infra`)
- Branch names reference the issue number: `feature/GH-42-feed-reconnection`
- Commit messages reference the issue: `edge: add feed reconnection (#42)`
- PRs reference the issue with closing keyword: `Closes #42`
- **If there is no issue, the work does not start.** Create the issue first, then create the branch.

### Clean repo policy (ZERO TOLERANCE)

**The repo must be clean at all times.** This means:

1. **No uncommitted changes left behind.** Before stopping work: commit, stash, or discard. Never leave a dirty working tree.
2. **No untracked files.** If you create a file, it gets committed or added to `.gitignore`. Nothing floats.
3. **No broken builds on `develop`.** If your merge breaks the build, you fix it immediately or revert.
4. **No force pushes** to `main` or `develop`. Ever.
5. **No merge commits on feature branches.** Rebase feature branches onto `develop` before merging. Keep history linear and clean.
6. **No WIP commits on shared branches.** Squash or rewrite before pushing to `develop`.
7. **`git status` must return clean** before any branch switch, any merge, any push. Check every time.

### Commit conventions

- Format: `<scope>: <imperative verb> <what> (#<issue>)` — e.g., `edge: add feed reconnection with exponential backoff (#42)`
- Scopes: `common`, `nitro`, `edge`, `hive`, `workload`, `telemetry`, `oracle`, `fault`, `report`, `scenario`, `docgen`, `testenv`, `ci`, `docs`, `rfc`
- Sign all commits: `git config commit.gpgsign true`
- Multi-line body for non-trivial changes: what changed, why, and what issue it closes
- **Every commit must reference a GitHub issue number.** No orphan commits.

## DNA provenance (do not skip)

Every source file must include the license header:
```rust
// Copyright (c) 2025-present Don Johnson
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://github.com/copyleftdev/gashammer/blob/main/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
```

Every binary must embed the canary string (gashammer-common/src/build_info.rs). Every metric must use the `gashammer_` prefix. Every error code must use the `GH-Exxx` scheme.

## When in doubt

1. Check the RFC first.
2. If the RFC doesn't cover it, check how Nitro itself does it (https://github.com/OffchainLabs/nitro).
3. Prefer boring, correct code over clever code.
4. When adding a dependency, check crates.io download count, last publish date, and whether alloy/tokio already expose the functionality.
5. If a design decision isn't in an RFC and seems significant, write it down as a new RFC or add a section to an existing one.
