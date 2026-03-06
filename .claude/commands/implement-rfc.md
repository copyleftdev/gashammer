---
description: Implement a feature from an RFC specification
---

Implement the feature described in RFC $ARGUMENTS.

Steps:

1. Read `rfcs/RFC-$ARGUMENTS*.md` to understand the full specification.

2. Identify which crates are affected. Check if they exist; if not, use `/new-crate` first.

3. For each affected crate:
   a. Read the existing code to understand current state.
   b. Write tests FIRST based on the RFC's data structures and behavior descriptions.
   c. Implement the minimum code to make tests pass.
   d. Run `cargo test -p <crate>` to verify.
   e. Run `cargo clippy -p <crate> -- -D warnings` to verify no warnings.

4. If the RFC specifies gRPC services, update `proto/` definitions and regenerate.

5. If the RFC specifies REST endpoints, add them to the hive's axum router.

6. If the RFC specifies configuration, add the config structs to `gashammer-common`.

7. Run `cargo test --workspace` to verify nothing is broken.

8. Run `cargo clippy --workspace --all-targets -- -D warnings`.

9. Summarize what was implemented, what tests were added, and any deviations from the RFC. If there are deviations, update the RFC.
