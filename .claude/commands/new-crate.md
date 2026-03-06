---
description: Scaffold a new GasHammer workspace crate with license header, error types, and tests
---

Create a new crate in the GasHammer workspace. The crate name will be provided as $ARGUMENTS.

Steps:

1. Create `crates/gashammer-$ARGUMENTS/Cargo.toml` with:
   - `name = "gashammer-$ARGUMENTS"`
   - `version = "0.1.0"`
   - `edition = "2021"`
   - `rust-version = "1.75.0"`
   - `license = "BSL-1.1"`
   - `repository = "https://github.com/copyleftdev/gashammer"`
   - Default deps: `thiserror`, `tracing`, `serde` with derive feature
   - `[dev-dependencies]`: `tokio` with test-util, `proptest`

2. Create `crates/gashammer-$ARGUMENTS/src/lib.rs` with:
   - The BSL-1.1 license header (copy exactly from CLAUDE.md § "DNA provenance")
   - Module-level doc comment: `//! GasHammer $ARGUMENTS crate.`
   - A `pub mod error;` declaration
   - Re-export: `pub use error::Error;`

3. Create `crates/gashammer-$ARGUMENTS/src/error.rs` with:
   - License header
   - A `thiserror`-based `Error` enum with at least one variant
   - Use the `GH-Exxx` error code scheme in Display impls

4. Add the crate to the workspace `Cargo.toml` members list.

5. Run `cargo check -p gashammer-$ARGUMENTS` to verify it compiles.
