---
description: Create a new RFC following GasHammer template and conventions
---

Create a new RFC for the topic described in $ARGUMENTS.

Steps:

1. Determine the next RFC number by listing existing files in `rfcs/`.

2. Create `rfcs/RFC-<NNNN>-<kebab-case-topic>.md` with this structure:
   - `# RFC-<NNNN>: <Title>`
   - `## Status` → Draft
   - `## Author` → Don Johnson
   - `## Depends On` → list relevant prior RFCs
   - `## Summary` → one paragraph
   - Numbered sections covering: Context, Decision, Data Structures (with Rust code), Integration Points, Configuration, Risks, Open Questions, References
   - Rust code blocks must include realistic types with derives, fields, and doc comments
   - Reference specific sections of other RFCs where integration occurs

3. Update the RFC table in `CLAUDE.md` to include the new RFC.

4. Verify the file renders cleanly (no broken markdown).
