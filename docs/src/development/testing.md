# Testing

GasHammer has three test tiers: unit tests, integration tests, and end-to-end lifecycle tests.

## Unit Tests

Unit tests live in `mod tests` blocks at the bottom of each source file.

```bash
# All unit tests
cargo test --workspace

# Single crate
cargo test -p gashammer-nitro

# Single test by name
cargo test --workspace -- test_feed_reconnects
```

### Conventions

- Test names describe behavior, not implementation: `test_feed_reconnects_after_disconnect`, not `test_feed_1`.
- Async tests use `#[tokio::test]`. Never `block_on`.
- Use `proptest` for property-based tests on serialization round-trips and workload generation.
- Oracle invariant checks must have both passing and failing test cases.
- Do not test private implementation details --- test the public contract.

### Current Test Counts

| Crate | Tests |
|-------|-------|
| gashammer-common | 30+ |
| gashammer-nitro | 48 |
| gashammer-edge | 30+ |
| gashammer-hive | 65 |
| gashammer-workload | 40+ |
| gashammer-telemetry | 20+ |
| gashammer-oracle | 30+ |
| gashammer-fault | 35+ |
| gashammer-report | 30+ |
| gashammer-scenario | 25+ |
| gashammer-docgen | 15+ |
| gashammer-testenv | 10+ |

## Integration Tests

Integration tests require Docker and a running Nitro devnet. They are gated by the `integration` feature flag.

```bash
# Run integration tests (requires Docker)
cargo test --test integration -- --test-threads=1
```

**Location:** `crates/gashammer-nitro/tests/integration.rs` and `crates/gashammer-testenv/tests/smoke.rs`.

### What They Test

- Real JSON-RPC connectivity to a Nitro sequencer.
- Transaction submission and receipt polling.
- Contract deployment and interaction.
- Feed relay connectivity.
- L1 Geth health checks.

### Devnet Profiles

| Profile | Containers | Use Case |
|---------|------------|----------|
| `Minimal` | L1 Geth, Sequencer | Fast smoke tests |
| `Standard` | L1 Geth, Sequencer, Feed Relay | Integration tests |
| `Full` | All + contract deployment | E2E lifecycle tests |

## E2E Lifecycle Tests

Full lifecycle tests boot a devnet, deploy contracts, fund accounts, run a scenario, and verify the report.

```bash
GASHAMMER_TEST_PROFILE=standard cargo test --test e2e -- --test-threads=1
```

### LifecycleTest Builder

```rust
LifecycleTestBuilder::new()
    .scenario_yaml(include_str!("scenario.yaml"))
    .edge_count(2)
    .deploy_contracts(true)
    .expected_outcome(ExpectedOutcome::Pass)
    .timeout(Duration::from_secs(120))
    .build()
    .run()
    .await;
```

### Test Contracts

GasHammer ships 7 compiled Solidity test contracts in `contracts/test/build/`:

| Contract | Purpose |
|----------|---------|
| `Counter` | Increment/decrement with storage writes |
| `GasBurner` | Configurable gas consumption loop |
| `StorageWriter` | Batch storage slot writes |
| `EventEmitter` | Emit events for log testing |
| `Reverter` | Controlled reverts for failure testing |
| `ContentionTarget` | Concurrent access patterns |
| `GasHammerERC20` | ERC-20 token for transfer testing |

Contracts are pre-compiled. Bytecode is embedded at build time via `include_bytes!`.

## CI Pipeline

GitHub Actions runs the full test suite on every PR:

```yaml
jobs:
  check:
    - cargo fmt --all -- --check
    - cargo clippy --workspace --all-targets -- -D warnings
    - cargo test --workspace
    - cargo deny check licenses
    - scripts/check-license-headers.sh
```

All checks must pass before merge. No exceptions.

## Writing New Tests

1. Add a `#[test]` or `#[tokio::test]` function in the `mod tests` block of the file containing the code under test.
2. Name the test `test_<behavior_description>`.
3. For integration tests that need Docker, add them to the `tests/` directory and gate with `#[cfg(feature = "integration")]`.
4. Run `cargo test -p <crate>` to verify locally before pushing.
