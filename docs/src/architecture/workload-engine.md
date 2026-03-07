# Workload Engine

The workload engine generates transaction load at a target gas rate. Workloads are defined by gas/sec, not requests/sec --- this accurately models blockchain resource consumption since different transaction types consume vastly different amounts of gas.

Ref: RFC-0005.

## Gas Rate Modes

The engine supports four gas rate modes, each computing the target gas/sec for a given elapsed time.

| Mode | Fields | Behavior |
|------|--------|----------|
| `Sustained` | `gas_per_sec` | Constant rate |
| `Ramped` | `start_gas_per_sec`, `end_gas_per_sec`, `duration_ms` | Linear interpolation between start and end |
| `Bursty` | `baseline_gas_per_sec`, `burst_peak_gas_per_sec`, `burst_duration_ms`, `burst_interval_ms` | Periodic bursts above a baseline |
| `Custom` | `schedule: Vec<(u64, u64)>` | Arbitrary piecewise-linear schedule |

The engine evaluates the rate on each tick (`TICK_INTERVAL_MS`) and generates enough transactions to consume the target gas for that interval.

## Transaction Templates

Templates define the shape of a transaction. Each template implements the `Parameterizer` trait:

```rust
trait Parameterizer: Send + Sync {
    fn descriptor(&self) -> TemplateDescriptor;
    fn next(&mut self, rng: &mut impl Rng, to: Address) -> TemplateOutput;
}
```

Every call to `next()` returns a `TemplateOutput` with `to`, `value`, `data`, `gas_limit`, `estimated_gas`, and `template_name`.

### Built-in Templates

| Template | Est. Gas | Description |
|----------|----------|-------------|
| `simple-transfer` | 21,000 | ETH value transfer |
| `noop` | 21,000 | Zero-value self-transfer |
| `erc20-transfer` | ~65,000 | ERC-20 `transfer()` |
| `erc20-approve` | ~46,000 | ERC-20 `approve()` |
| `storage-write` | ~44,000 | Single storage slot write |
| `storage-write-heavy` | ~200,000 | Multiple storage slot writes |
| `compute-heavy` | ~200,000 | CPU-intensive loop |
| `calldata-heavy` | configurable | Large calldata payload |

All calldata includes the `CALLDATA_MAGIC` bytes `[0x47, 0x48, 0x4D, 0x52]` ("GHMR") for on-chain attribution.

### Template Weighting

Scenarios assign integer weights to templates. The engine normalizes weights into a probability distribution and selects templates proportionally.

```yaml
templates:
  - name: simple-transfer
    weight: 70
  - name: erc20-transfer
    weight: 30
```

This produces ~70% transfers and ~30% ERC-20 calls by count, though the gas distribution will differ due to different per-tx gas costs.

## Account Pool

Test accounts are derived deterministically from an HD wallet mnemonic.

**Derivation path:** `m/44'/60'/0'/0'/0x4748/{index}`

The `0x4748` segment is the GasHammer DNA marker ("GH" in hex). This makes GasHammer-generated accounts identifiable on-chain.

**Key properties:**

- Deterministic: same mnemonic + same index = same private key.
- Partitioned: accounts are divided across edges with no overlap.
- Nonce-tracking: each `Account` maintains a local nonce counter, avoiding on-chain queries. On tx failure, the nonce is not incremented.
- Thread-safe: `Account::next_nonce()` uses `AtomicU64` for lock-free increment.

## Deterministic PRNG

All randomness in workload generation (template selection, parameter variation, account assignment) flows through a seeded `ChaCha8Rng`. The same `seed` value with the same scenario configuration produces the same transaction sequence, enabling reliable comparison across runs.

The seed is set in the scenario:

```yaml
workload:
  seed: 42
```

Default seed is `0` if omitted.
