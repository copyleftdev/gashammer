---
description: Run the full verification suite — build, test, lint, format check
---

Run the complete GasHammer verification pipeline.

// turbo
1. `cargo fmt --all -- --check`

// turbo
2. `cargo clippy --workspace --all-targets -- -D warnings`

// turbo
3. `cargo test --workspace`

// turbo
4. `cargo deny check licenses`

5. Report results as a table:

| Check | Status |
|-------|--------|
| Format | pass/fail |
| Clippy | pass/fail |
| Tests | pass/fail (N passed, N failed) |
| Licenses | pass/fail |

If any check failed, show the relevant error output and suggest a fix.
