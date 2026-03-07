# RFC-0011: DNA & Provenance Strategy

## Status

Draft

## Author

Don Johnson

## Summary

GasHammer will be released as open source. Open source does not mean unprotected. This RFC defines a **multi-layer provenance and attribution system** — what we call "DNA" — embedded from the wire protocol level up through the application, artifacts, documentation, and community. The goal is not to restrict use, but to make the origin of this work **undeniable, traceable, and legally defensible** even when someone actively tries to strip attribution.

This is not DRM. This is provenance.

---

## 1. Core Philosophy: Two-Tier Provenance

The single most important insight in this strategy:

> **If they can see it, they can remove it. The real DNA is the DNA they don't know exists.**

All provenance mechanisms are classified into two tiers:

### Tier 1 — Surface Layer (Deterrent)

Visible, professional, expected. These exist for **legitimate users and legal standing**, not to stop a determined thief. We expect them to be stripped by bad actors. That's fine — *the act of stripping is itself evidence of intent*.

- License file and per-file headers
- `gashammer version` command output
- One startup log line with copyright
- Small report footer (standard for any tool)

Keep this layer **minimal and tasteful**. No aggressive branding. No forced banners that can't be suppressed. The surface layer should feel like any well-maintained open-source project — nothing that screams "I'm watching you."

### Tier 2 — Deep Layer (Forensic)

Invisible during normal use. Not documented in user-facing materials. **Not mentioned in the README, not mentioned in `--help` output, not mentioned in the mdBook.** These exist exclusively for forensic tracing when you suspect theft. A thief would need to:

- Reverse-engineer the binary to find canary strings
- Inspect raw Parquet file footers that no one ever looks at
- Decode transaction calldata byte-by-byte to find magic suffixes
- Understand HD wallet derivation internals
- Audit every Prometheus metric name for namespace patterns
- Trace error code namespaces through their own Stack Overflow posts
- Regenerate all gRPC clients from scratch to change the package name
- Rewrite all config keys, breaking their deployment automation

The deep layer's power is **cumulative and high-friction**. Each individual fingerprint is small. Removing any one is trivial. Removing *all of them* requires understanding every layer of the system and rewriting significant infrastructure. At that point, they've built their own tool.

### What Goes Where

| Mechanism | Tier | Why |
| --- | --- | --- |
| License file | Surface | Legal standing; expected to be stripped |
| Version command | Surface | Standard CLI behavior |
| Startup log line | Surface | Professional; one line |
| Report footer | Surface | Small, tasteful |
| Binary canary string | **Deep** | Buried in `.rodata`; not referenced by any output |
| Calldata magic bytes | **Deep** | On-chain; requires tx builder internals knowledge |
| HD derivation path | **Deep** | Math-level; invisible in output |
| Parquet metadata | **Deep** | Nobody inspects Parquet footers |
| OCI image labels | **Deep** | Survives registry pushes; rarely inspected |
| Metric namespace | **Deep** | Propagates into dashboards, alerts, runbooks |
| Error code namespace | **Deep** | Propagates into issue trackers, wikis, SO posts |
| gRPC package name | **Deep** | Embedded in all generated client code |
| Config key vocabulary | **Deep** | Propagates into Helm values, Terraform, Ansible |
| Signed releases | Surface | Standard security practice |
| Git history | **Deep** | Unforgeable organic development record |
| Trademark | Surface | Legal deterrent |
| Concept anchoring | **Deep** | Timestamped public records predate any copy |

---

## 2. Threat Model

| Threat | Description | Severity |
| --- | --- | --- |
| **License stripping** | Someone removes the license file and claims the work | High |
| **Rebrand fork** | Someone forks, renames, removes credits, and presents as original | High |
| **Selective extraction** | Someone copies key algorithms/modules into a proprietary tool | Medium |
| **Cloud-as-a-service** | Someone wraps GasHammer in a SaaS offering without attribution | High |
| **Academic theft** | Someone publishes the architecture as their own research | Medium |

The DNA strategy must make all five traceable via **Tier 2 (deep) mechanisms** — not by relying on surface attribution that a thief will strip.

---

## 3. Layer 0 — License Selection (Tier 1: Surface)

### 3.1 Recommendation: Business Source License 1.1 (BSL 1.1)

BSL 1.1 is the same license used by **Offchain Labs for Nitro itself**, as well as Uniswap and Aave. This is strategically important:

- The interviewing company already operates under BSL — they understand and respect it
- BSL allows full source availability and use for non-production purposes immediately
- After a **Change Date** (e.g., 3 years), the code converts to a permissive license (Apache 2.0 or MIT)
- The **Additional Use Grant** can permit specific uses (e.g., internal testing, non-commercial use)

```text
License:          Business Source License 1.1
Licensor:         Don Johnson
Licensed Work:    GasHammer <version>
Change Date:      <3 years from release>
Change License:   Apache License 2.0
Additional Use Grant:
  You may use the Licensed Work for non-commercial purposes,
  internal testing and evaluation, academic research, and
  contributions back to the Licensed Work, without restriction.
```

### 3.2 Why Not AGPL/GPL/MIT/Apache

| License | Problem |
| --- | --- |
| **MIT/Apache** | Zero protection against SaaS repackaging or rebrand forks |
| **GPL** | Copyleft is strong but doesn't cover SaaS (network use loophole) |
| **AGPL** | Closes the SaaS loophole but is toxic to enterprise adoption; Offchain Labs uses BSL, not AGPL |
| **Proprietary** | Defeats the purpose of open source; limits community |
| **BSL 1.1** | Source-available with time-delayed open-source conversion; best of both worlds |

### 3.3 Per-File License Headers

Every source file includes a license header:

```rust
// Copyright (c) 2025-present Don Johnson
// Licensed under the Business Source License 1.1 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     https://github.com/copyleftdev/gashammer/blob/main/LICENSE
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
```

Automated via `cargo-deny` license check + CI enforcement. **No file ships without a header.**

---

## 4. Layer 1 — Wire Protocol DNA (Tier 2: Deep)

### 4.1 gRPC Service Namespace

The gRPC service uses a globally unique, registered package name:

```protobuf
package gashammer.v1;

// This protocol is part of the GasHammer project
// Copyright (c) 2025-present Don Johnson
// https://github.com/copyleftdev/gashammer
```

The package name `gashammer.v1` is a fingerprint. Anyone using our proto definitions carries it in every gRPC frame.

### 4.2 User-Agent String (Tier 2: Deep)

All hive REST API responses include custom headers:

```
X-Powered-By: GasHammer/<version>
X-GasHammer-Run-Id: <uuid>
X-GasHammer-Build: <git-sha>
```

These headers appear in logs, traces, and proxy records everywhere GasHammer traffic flows.

### 4.3 WebSocket Feed Handshake (Tier 2: Deep)

All outbound HTTP requests from edges and hive include a distinctive User-Agent:

```
User-Agent: GasHammer/<version> (https://github.com/copyleftdev/gashammer; <build-sha>)
```

This appears in:
- Target RPC node access logs
- Feed relay connection logs
- L1 node access logs
- Any intermediate proxy or WAF logs

Every system GasHammer touches records its name.

### 4.4 gRPC Metadata (Tier 2: Deep)

The feed relay WebSocket connection includes the User-Agent at handshake. Additionally, the edge sends an initial identification frame:

```json
{
  "client": "gashammer-edge",
  "version": "0.1.0",
  "edge_id": "<uuid>",
  "run_id": "<uuid>"
}
```

### 4.5 gRPC Call Metadata (Tier 2: Deep)

Every gRPC call between hive and edge carries metadata headers:

```text
x-gashammer-version: 0.1.0
x-gashammer-build: abc123def
x-gashammer-edge-id: <uuid>
```

These propagate into gRPC interceptor logs on both sides. A fork that reuses the proto definitions inherits the metadata pattern.

---

## 5. Layer 2 — Transaction DNA (Tier 2: Deep)

### 5.1 Transaction Calldata Watermark

GasHammer-generated transactions carry a subtle but detectable fingerprint in their calldata structure. For simple transfers and contract calls, the last 4 bytes of calldata (when present) encode a GasHammer identifier:

```rust
const GASHAMMER_MAGIC: [u8; 4] = [0x47, 0x48, 0x4D, 0x52]; // "GHMR" in ASCII
```

This is appended to calldata for tx templates that don't have strict ABI requirements (e.g., `storage-write`, `compute-heavy`). For strict ABI calls (ERC-20, DeFi), the magic is encoded in an unused portion of the input data or omitted.

**Forensic value:** If someone claims they built an independent load tester, but their on-chain transactions all end with `0x47484D52`, the origin is traceable.

### 5.2 Account Derivation Path

GasHammer derives test accounts using a deterministic HD path:

```
m/44'/60'/0'/0'/0x4748/{index}
```

The `0x4748` ("GH") path component is non-standard and distinctive. Any on-chain forensics can identify GasHammer-derived accounts by their derivation pattern if the master key is known.

### 5.3 Nonce Patterns

GasHammer's deterministic PRNG produces a characteristic distribution of gas limits, values, and timing patterns that are statistically distinguishable from other load generators. This is a natural byproduct of the deterministic engine (RFC-0005), not an intentional watermark — but it serves as one.

---

## 6. Layer 3 — Artifact DNA (Mixed Tiers)

### 6.1 Report Footer (Tier 1: Surface)

Every report (JSON, Markdown, HTML) includes a provenance block:

```json
{
  "provenance": {
    "generator": "GasHammer",
    "generator_version": "0.1.0",
    "generator_build": "abc123def",
    "generator_url": "https://github.com/copyleftdev/gashammer",
    "copyright": "Copyright (c) 2025-present Don Johnson",
    "license": "BSL-1.1",
    "report_id": "<uuid>",
    "generated_at": "2025-03-06T13:21:00Z",
    "signature": "<ed25519-signature-of-report-contents>"
  }
}
```

Markdown reports include a footer:

```markdown
---
*Generated by [GasHammer](https://github.com/copyleftdev/gashammer) v0.1.0 — Adversarial load testing for rollup infrastructure.*
*Copyright © 2025-present Don Johnson. Licensed under BSL-1.1.*
```

HTML reports include `<meta>` tags:

```html
<meta name="generator" content="GasHammer v0.1.0">
<meta name="author" content="Don Johnson">
<meta name="copyright" content="Copyright (c) 2025-present Don Johnson">
```

### 6.2 Parquet Metadata (Tier 2: Deep)

Raw telemetry Parquet files include custom metadata in the file footer:

```
gashammer.version = "0.1.0"
gashammer.build = "abc123def"
gashammer.run_id = "<uuid>"
gashammer.copyright = "Copyright (c) 2025-present Don Johnson"
```

Parquet metadata survives file copies, S3 transfers, and most data processing pipelines.

### 6.3 Scenario Compilation Signature (Tier 2: Deep)

Compiled scenario artifacts include a provenance section:

```json
{
  "compiled_by": "gashammer-scenario",
  "compiler_version": "0.1.0",
  "source_hash": "<sha256>",
  "compilation_signature": "<ed25519-sig>"
}
```

### 6.4 Docker Image Labels (Tier 2: Deep)

Container images include OCI labels:

```dockerfile
LABEL org.opencontainers.image.title="GasHammer"
LABEL org.opencontainers.image.description="Adversarial load testing for rollup infrastructure"
LABEL org.opencontainers.image.authors="Don Johnson"
LABEL org.opencontainers.image.url="https://github.com/copyleftdev/gashammer"
LABEL org.opencontainers.image.source="https://github.com/copyleftdev/gashammer"
LABEL org.opencontainers.image.licenses="BSL-1.1"
LABEL org.opencontainers.image.vendor="Don Johnson"
LABEL com.gashammer.build.sha="${GIT_SHA}"
LABEL com.gashammer.build.version="${VERSION}"
```

These labels are queryable via `docker inspect` and survive pushes to any OCI registry.

### 6.5 Helm Chart Annotations (Tier 2: Deep)

```yaml
# Chart.yaml
annotations:
  category: performance-testing
  author: Don Johnson
  license: BSL-1.1
  source: https://github.com/copyleftdev/gashammer
```

---

## 7. Layer 4 — Binary DNA (Mixed Tiers)

### 7.1 Build-Time Canary (Tier 2: Deep)

Every compiled binary embeds provenance at compile time:

```rust
// gashammer-common/src/build_info.rs
pub const GASHAMMER_NAME: &str = "GasHammer";
pub const GASHAMMER_VERSION: &str = env!("CARGO_PKG_VERSION");
pub const GASHAMMER_BUILD_SHA: &str = env!("GASHAMMER_BUILD_SHA");
pub const GASHAMMER_BUILD_TIME: &str = env!("GASHAMMER_BUILD_TIME");
pub const GASHAMMER_COPYRIGHT: &str = "Copyright (c) 2025-present Don Johnson";
pub const GASHAMMER_LICENSE: &str = "BSL-1.1";
pub const GASHAMMER_URL: &str = "https://github.com/copyleftdev/gashammer";

// Canary string that survives in the binary even after stripping
pub const GASHAMMER_CANARY: &str = concat!(
    "GasHammer — Adversarial load testing for rollup infrastructure. ",
    "Copyright (c) 2025-present Don Johnson. ",
    "https://github.com/copyleftdev/gashammer. ",
    "Licensed under BSL-1.1."
);
```

The `GASHAMMER_CANARY` is a static string embedded in the binary's `.rodata` section. It survives:
- Binary stripping (`strip --strip-all`)
- UPX compression
- Container layer squashing
- Most obfuscation attempts

Detectable via: `strings gashammer-hive | grep GasHammer`

### 7.2 Version Command (Tier 1: Surface)

```text
$ gashammer version
GasHammer v0.1.0
Build: abc123def (2025-03-06T13:21:00Z)
Copyright (c) 2025-present Don Johnson
License: BSL-1.1
Source: https://github.com/copyleftdev/gashammer
```

This output is produced by every binary (hive, edge) and cannot be suppressed via flags.

### 7.3 Startup Log Line (Tier 1: Surface)

On startup, every GasHammer process logs:

```
[INFO] GasHammer v0.1.0 (abc123def) starting
[INFO] Copyright (c) 2025-present Don Johnson — BSL-1.1
[INFO] https://github.com/copyleftdev/gashammer
```

This appears in every log file, every container log stream, every systemd journal entry.

---

## 8. Layer 5 — Structural DNA (Tier 2: Deep)

### 8.1 Unique Terminology

GasHammer introduces domain-specific terminology that acts as a linguistic fingerprint:

| Term | Meaning | Searchability |
| --- | --- | --- |
| **Hive** | Control plane | Common in some contexts but specific in our combination |
| **Edge** | Regional worker | Common but specific in our hive/edge pairing |
| **Safe Operating Envelope** | Measured capacity range | Distinctive phrase |
| **Gas-weighted workload** | Tx profile by gas, not request count | Distinctive concept |
| **Hive Edge architecture** | The distributed model name | Distinctive compound term |
| **Decision-grade outputs** | Report quality standard | Distinctive phrase |
| **Correctness Oracle** | Verification subsystem | Distinctive in this context |
| **Feed Correlator** | Tx-to-feed matching engine | Distinctive component name |
| **Fault Journal** | Persistent fault recovery log | Distinctive name |
| **Gas Hammer** (the name itself) | The product | Highly distinctive, searchable, trademarkable |

If someone copies the architecture and renames it, the structural concepts — "hive/edge topology with gas-weighted workloads, correctness oracles, and safe operating envelope reports" — are a unique combination that is searchable and attributable.

### 8.2 Metric Name Namespace

All GasHammer metrics use a distinctive prefix:

```
gashammer_edge_feed_messages_received
gashammer_edge_telemetry_dropped
gashammer_hive_telemetry_ingested
gashammer_run_gas_submitted_per_sec
gashammer_oracle_violations_total
```

The `gashammer_` prefix appears in:
- Prometheus metric names
- Grafana dashboards
- Alert rules
- Runbooks
- Blog posts describing monitoring

Anyone who copies the metrics carries the namespace.

### 8.3 Error Code Namespace

GasHammer errors use a distinctive code scheme:

```
GH-E001: Scenario validation failed
GH-E002: Edge registration rejected
GH-E003: Barrier synchronization timeout
GH-E100: Feed correlation failure
GH-E200: Oracle invariant violation
GH-E300: Fault injection failure
```

The `GH-Exxx` prefix is unique and searchable across codebases, Stack Overflow posts, issue trackers, and documentation.

### 8.4 Config Key Vocabulary

GasHammer config files use a distinctive structure and key names:

```toml
[hive]
listen_addr = "0.0.0.0:8080"
grpc_addr = "0.0.0.0:9090"

[edge.target]
sequencer_rpc = "..."
feed_relay = "ws://...:9642"
```

The combination of `[hive]`, `[edge.target]`, `sequencer_rpc`, and `feed_relay` in a TOML config is a distinctive pattern.

---

## 9. Layer 6 — Cryptographic Provenance (Tier 1: Surface)

### 9.1 Signed Releases

Every release is signed with an Ed25519 key:

```text
gashammer-v0.1.0-linux-amd64.tar.gz
gashammer-v0.1.0-linux-amd64.tar.gz.sig
gashammer-v0.1.0-linux-amd64.tar.gz.sha256
```

The signing key is:
- Published in the README
- Published in a `SECURITY.md` file
- Registered on keybase or equivalent
- Used consistently across all releases

### 9.2 Signed Artifacts

Run reports and compiled scenarios can optionally be signed:

```rust
pub struct SignedArtifact {
    pub content_hash: String,         // SHA-256 of content
    pub signature: String,            // Ed25519 signature
    pub signer: String,               // "gashammer-hive/<version>"
    pub public_key: String,           // verification key
    pub signed_at: DateTime<Utc>,
}
```

This creates a cryptographic chain of custody: if a report exists with a valid GasHammer signature, it was generated by GasHammer.

### 9.3 SBOM (Software Bill of Materials)

Every release includes a CycloneDX SBOM:

```text
gashammer-v0.1.0-sbom.json
```

The SBOM lists every dependency, its version, and its license. This:
- Proves the build composition
- Creates a verifiable snapshot of the dependency tree
- Is increasingly required for enterprise adoption (and shows engineering maturity)

Generated via `cargo-cyclonedx` in CI.

---

## 10. Layer 7 — Community & Documentation DNA (Mixed Tiers)

### 10.1 README Provenance (Tier 1: Surface)

The README includes:

```markdown
# GasHammer

**Adversarial load testing and distributed verification for rollup infrastructure.**

Created by [Don Johnson](https://github.com/<owner>).

> GasHammer measures how a chain behaves when the world hits it all at once.
```

The tagline and one-liner are distinctive and searchable.

### 10.2 RFC Provenance (Tier 2: Deep)

Every RFC carries author attribution (already present in all 10 RFCs). The RFC numbering scheme (`RFC-0001` through `RFC-0011`) and naming convention are distinctive.

### 10.3 CONTRIBUTORS File (Tier 1: Surface)

```markdown
# Contributors

## Original Author
Don Johnson — architecture, specification, implementation

## Contributors
(list grows as community contributes)
```

### 10.4 Git History (Tier 2: Deep)

The git history is the ultimate provenance record:
- First commit establishes creation date and author
- Commit signatures (GPG or SSH) prove authorship
- Continuous commit history proves organic development, not a bulk copy

**Sign every commit:**

```toml
# .gitconfig
[commit]
gpgsign = true
```

### 10.5 Concept Origin Anchors (Tier 2: Deep)

Publish key concepts externally before or at release to establish prior art:

- **Blog post** describing the "gas-weighted workload" concept
- **Conference talk** or meetup presentation on "safe operating envelopes for rollups"
- **Twitter/X thread** explaining the hive/edge architecture
- **arXiv preprint** (if applicable) on the adversarial load testing methodology

These create timestamped public records that predate any potential copy.

---

## 11. Layer 8 — Legal Infrastructure (Mixed Tiers)

### 11.1 Trademark (Tier 1: Surface)

"GasHammer" should be filed as a trademark for software tools in the blockchain/performance-testing category. This provides:
- Legal recourse against rebrand forks that keep the name
- Protection of the brand independent of the code

### 11.2 Copyright Registration (Tier 2: Deep)

Register the codebase with the U.S. Copyright Office (or equivalent). This:
- Establishes a legal record of creation date
- Enables statutory damages in infringement cases (up to $150,000 per work in the US)
- Costs ~$65 per registration

### 11.3 DCO (Developer Certificate of Origin)

Require all contributors to sign off commits via DCO:

```
Signed-off-by: Contributor Name <email>
```

This creates a legal chain of contribution provenance and is used by the Linux kernel, CNCF projects, and many major open-source projects.

Enforced via a CI check on every PR.

---

## 12. Detection and Monitoring

### 12.1 Automated Derivative Detection

Set up monitoring for unauthorized derivatives:

- **GitHub Code Search:** Periodic searches for distinctive strings (`GASHAMMER_CANARY`, `gashammer_`, `GH-E001`, `0x47484D52`)
- **Google Alerts:** Monitor for "safe operating envelope" + "rollup", "gas-weighted workload", "hive edge architecture"
- **Package Registry Monitoring:** Watch crates.io, npm, PyPI, Docker Hub for packages containing GasHammer code
- **Academic Search:** Monitor Google Scholar, arXiv for papers describing GasHammer's architecture

### 12.2 Fingerprint Verification Tool

Ship a built-in provenance verification command:

```text
$ gashammer provenance verify ./report.json
✓ Report signed by GasHammer v0.1.0 (abc123def)
✓ Signature valid (Ed25519)
✓ Content hash matches
✓ Generated at 2025-03-06T13:21:00Z
```

This lets anyone verify that an artifact was genuinely produced by GasHammer.

---

## 13. DNA Summary by Layer

| Layer | Mechanism | Survives Rename? | Survives Code Copy? | Detectable by `strings`? |
| --- | --- | --- | --- | --- |
| **0 License** | BSL-1.1 + per-file headers | No (strippable) | No | N/A |
| **1 Wire** | gRPC package, HTTP headers, User-Agent | Partially | Yes (if proto reused) | N/A |
| **2 Transaction** | Calldata magic bytes, HD path | Yes | Yes | Yes (on-chain) |
| **3 Artifact** | Report provenance, Parquet metadata, Docker labels | Partially | Yes | Yes |
| **4 Binary** | Canary string, build metadata, startup banner | Yes | Yes | Yes |
| **5 Structural** | Terminology, metric names, error codes, config schema | Yes | Yes | N/A (conceptual) |
| **6 Crypto** | Signed releases, signed artifacts, SBOM | No (strippable) | No | N/A |
| **7 Community** | Git history, blog posts, conference talks, README | Yes | Yes | N/A (external) |
| **8 Legal** | Trademark, copyright registration, DCO | Yes | Yes | N/A (legal) |

**Key insight:** No single layer is undefeatable. The power is in the **combination**. To fully strip GasHammer's DNA, an attacker would need to:

1. Remove all license headers from every file
2. Change the gRPC package namespace and regenerate all clients
3. Remove the calldata magic bytes and change the HD derivation path
4. Strip provenance from every report, Parquet file, and Docker image
5. Remove the canary string from the binary
6. Rename all metrics, error codes, config keys, and terminology
7. Rewrite the architecture description to avoid distinctive phrasing
8. Forge a new git history
9. Counter the trademark and copyright registration
10. Remove the DCO chain

At that point, they've rewritten the entire project. Which is the point.

---

## 13. Implementation Priority

### MVP (Before Release)

1. BSL-1.1 license file + per-file headers
2. Binary canary string + version command + startup banner
3. `gashammer_` metric prefix
4. `GH-Exxx` error codes
5. HTTP User-Agent + `X-Powered-By` header
6. Report provenance block
7. Git commit signing
8. CONTRIBUTORS file
9. README with clear attribution

### Post-MVP

1. Ed25519 artifact signing
2. Calldata magic bytes
3. HD derivation path
4. Docker OCI labels
5. Parquet metadata
6. SBOM generation
7. `gashammer provenance verify` command
8. Automated derivative detection monitoring

### Legal (Parallel Track)

1. Trademark filing
2. Copyright registration
3. DCO enforcement in CI
4. Public concept anchoring (blog post, talk)

---

## 14. Risks

### Risk: DNA mechanisms annoy legitimate users

Forced attribution footers, mandatory headers, or undiscoverable magic bytes could feel hostile.

**Mitigation:** All user-facing DNA (report footers, startup banners) is tasteful and professional. Hidden DNA (canary strings, calldata bytes, Parquet metadata) is invisible during normal use. The provenance serves users too — they can verify artifact authenticity.

### Risk: BSL-1.1 deters contributors

Some developers avoid non-OSI-approved licenses.

**Mitigation:** BSL has strong precedent (Nitro, Uniswap, CockroachDB, Sentry). The Change Date guarantees eventual full open-source. The Additional Use Grant permits internal testing and non-commercial use immediately.

### Risk: Trademark filing is expensive or complex

Trademark registration varies by jurisdiction and can cost $250–$1,000+.

**Mitigation:** Start with a US federal trademark application (USPTO TEAS Plus, ~$250). This covers the primary jurisdiction. International filing can follow if the project gains traction.

### Risk: Over-engineering provenance distracts from building the tool

**Mitigation:** MVP DNA mechanisms (§13) are simple: license headers, a canary string, metric prefixes, and a User-Agent. Total implementation effort: <1 day. Advanced mechanisms are post-MVP.

---

## 15. Open Questions

1. Should the BSL Change Date be 2 years or 3 years?
2. Should the Additional Use Grant explicitly name Offchain Labs as a permitted user?
3. Should the calldata magic bytes be configurable (allowing users to disable them for production-like testing)?
4. Should the provenance verification command be a separate binary or a subcommand of the main CLI?
5. What is the right balance between visible attribution (empowering) and hidden fingerprinting (defensive)?

---

## 16. References

- [Business Source License 1.1](https://mariadb.com/bsl11/)
- [Offchain Labs Nitro License (BSL-1.1)](https://github.com/OffchainLabs/nitro/blob/master/LICENSE)
- [CycloneDX SBOM Standard](https://cyclonedx.org/)
- [Developer Certificate of Origin](https://developercertificate.org/)
- [OCI Image Spec — Annotations](https://github.com/opencontainers/image-spec/blob/main/annotations.md)
- [USPTO TEAS Plus Application](https://www.uspto.gov/trademarks/apply)
