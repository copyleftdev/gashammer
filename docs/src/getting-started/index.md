# Getting Started

This guide walks you through installing GasHammer, configuring it for your Nitro deployment, and running your first load test.

## Prerequisites

- **Rust toolchain** 1.75.0 or later (`rustup` recommended)
- **Docker** for running integration tests and the Nitro devnet
- **A Nitro deployment** (or use the built-in devnet for local testing)

## Quick Start

```bash
# Clone the repository
git clone https://github.com/copyleftdev/gashammer.git
cd gashammer

# Build everything
cargo build --workspace

# Run unit tests
cargo test --workspace

# Run with the local devnet (requires Docker)
cargo test --test integration -- --test-threads=1
```

## Sections

- [Installation](./installation.md) --- detailed build and install instructions
- [Configuration](./configuration.md) --- configure hive, edges, and scenarios
- [First Run](./first-run.md) --- run your first load test end-to-end
