# Contributing

## Prerequisites

- Rust 1.75.0+ (install via [rustup](https://rustup.rs/))
- Docker 20.10+ (for integration and E2E tests)
- `protoc` (Protocol Buffers compiler, for gRPC code generation)
- `mdbook` (for documentation: `cargo install mdbook`)
- `cargo-deny` (for license checks: `cargo install cargo-deny`)

## Development Workflow

```bash
# Build everything
cargo build --workspace

# Type-check only (faster)
cargo check --workspace

# Format
cargo fmt --all

# Lint
cargo clippy --workspace --all-targets -- -D warnings

# Test
cargo test --workspace

# License check
cargo deny check licenses
```

## Gitflow

This project uses strict gitflow. Every rule is non-negotiable.

### Branch Model

```text
main ───────────────────────────────── tagged releases only
  └── develop ──────────────────────── integration branch
        ├── feature/GH-<#>-<desc> ── new work
        ├── fix/GH-<#>-<desc> ────── bug fixes
        ├── release/<version> ────── release prep
        └── hotfix/GH-<#>-<desc> ── emergency fixes
```

### Rules

1. **Never commit directly to `main`.** All changes reach `main` through release branches.
2. **All feature and fix branches merge to `develop`** via PR with squash merge.
3. **Release branches** merge to both `main` (with tag) and `develop`.
4. **Hotfix branches** branch from `main`, merge to both `main` and `develop`.
5. **Delete branches after merge.** No stale branches.
6. **No force pushes** to `main` or `develop`.
7. **Rebase feature branches** onto `develop` before merging.

### Commit Format

```
<scope>: <imperative verb> <what> (#<issue>)
```

**Scopes:** `common`, `nitro`, `edge`, `hive`, `workload`, `telemetry`, `oracle`, `fault`, `report`, `scenario`, `docgen`, `testenv`, `ci`, `docs`, `rfc`

**Examples:**

```
nitro: add L1 monitor background poll loop (#83)
fault: fix auto-clear bug in FaultManager (#82)
docs: rewrite mdBook documentation (#89)
```

Every commit must reference a GitHub issue number.

## GitHub Issues

**No work happens without an issue.** Every feature, bug fix, refactor, and documentation change requires an issue first.

- **Title format:** `[<scope>] <imperative description>`
- **Required fields:** description, acceptance criteria, labels
- **Branch name:** `feature/GH-<issue#>-<short-description>`
- **PR closing keyword:** `Closes #<issue#>`

## Code Conventions

### Error Handling

- `thiserror` for library errors, `anyhow` only in binaries and tests.
- Never `unwrap()` in library code.
- Never `expect()` without a message explaining the invariant.

### Async

- `tokio` only. No `async-std`.
- Never `tokio::spawn` without storing the `JoinHandle`.
- No `sleep` for synchronization --- use channels, barriers, or `Notify`.

### Naming

- Crates: `gashammer-{name}` in `Cargo.toml`, `gashammer_{name}` as Rust modules.
- No abbreviations in public APIs except: `tx`, `rpc`, `ws`, `config`.

### Visibility

- Default to `pub(crate)`. Only `pub` items that are part of the API contract.

### Imports

Group in order, separated by blank lines:

1. `std`
2. External crates
3. Workspace crates
4. `crate`/`super`/`self`

### Testing

- Every public function gets a test.
- `#[tokio::test]` for async tests. Never `block_on`.
- Test names describe behavior: `test_feed_reconnects_after_disconnect`.
- No `#[allow(dead_code)]` --- delete dead code instead.
- No `println!` --- use `tracing`.

## License Header

Every `.rs` file must include:

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

Enforced by CI via `scripts/check-license-headers.sh`.
