---
description: Add a test for a specific function or module with proper GasHammer conventions
---

Add tests for `$ARGUMENTS`.

Steps:

1. Read the source file containing `$ARGUMENTS` to understand the function/module behavior.

2. Check the relevant RFC for expected behavior, edge cases, and invariants.

3. Write tests following these rules:
   - Test name describes behavior: `test_<thing>_<condition>_<expected_result>`
   - Use `#[tokio::test]` for async code
   - Include: one success path, one failure/error path, one edge case minimum
   - For serialization types: add a `proptest` round-trip test
   - For public API functions: test the contract, not internals
   - Never use `unwrap()` in test assertions — use `assert!`, `assert_eq!`, or `?` with `Result` return

4. Run `cargo test -p <crate> -- <test_name>` to verify each test passes.

5. Run `cargo test -p <crate>` to verify nothing else broke.
