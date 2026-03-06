# Installation

## From Source

GasHammer is built with Cargo. Clone and build:

```bash
git clone https://github.com/copyleftdev/gashammer.git
cd gashammer
cargo build --workspace --release
```

The release binaries are placed in `target/release/`:

- `gashammer-hive` --- the control plane binary
- `gashammer-edge` --- the edge worker binary

## System Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 2 cores | 4+ cores |
| RAM | 2 GB | 8 GB |
| Disk | 1 GB | 10 GB (for telemetry data) |
| Network | 100 Mbps | 1 Gbps |
| Rust | 1.75.0 | Latest stable |
| Docker | 20.10+ | Latest |

## Docker Images

Pre-built container images are available from GitHub Container Registry:

```bash
docker pull ghcr.io/copyleftdev/gashammer-hive:latest
docker pull ghcr.io/copyleftdev/gashammer-edge:latest
```

## Verifying the Installation

After building, verify the binary includes the GasHammer DNA canary:

```bash
strings target/release/gashammer-hive | grep GASHAMMER_CANARY
```

Check the version and build metadata:

```bash
./target/release/gashammer-hive --version
```

## Development Setup

For development, install additional tools:

```bash
# Install mdBook for documentation
cargo install mdbook

# Install cargo-deny for license checking
cargo install cargo-deny

# Verify the full CI suite passes locally
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
