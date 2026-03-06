---
description: Verify all DNA provenance markers are present across the codebase
---

Audit the GasHammer codebase for DNA provenance compliance per RFC-0011.

Steps:

// turbo
1. Check license headers exist in all `.rs` files:
   `find crates/ xtask/ -name "*.rs" -exec grep -L "Business Source License 1.1" {} \;`
   Any files listed are MISSING the license header. Fix them.

// turbo
2. Check canary string is defined:
   `grep -r "GASHAMMER_CANARY" crates/gashammer-common/src/`

// turbo
3. Check metric prefix compliance:
   `grep -rn "metrics::" crates/ --include="*.rs" | grep -v "gashammer_" | grep -v "test" | grep -v "//"`
   Any metric registrations not using the `gashammer_` prefix are violations.

// turbo
4. Check error code compliance:
   `grep -rn "GH-E" crates/ --include="*.rs" | head -20`
   Verify error codes follow the `GH-Exxx` scheme.

// turbo
5. Check User-Agent is set:
   `grep -rn "User-Agent" crates/ --include="*.rs"`

6. Report results as a compliance table:

| Check | Status | Files Affected |
|-------|--------|----------------|
| License headers | pass/fail | list |
| Canary string | present/missing | - |
| Metric prefix | pass/N violations | list |
| Error codes | pass/N violations | list |
| User-Agent | present/missing | - |
