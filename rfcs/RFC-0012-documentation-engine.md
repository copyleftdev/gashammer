# RFC-0012: Documentation Engine

## Status

Draft

## Author

Don Johnson

## Depends On

- RFC-0002 (Technology Stack & Dependencies)
- RFC-0003 (Hive Control Plane — REST API)
- RFC-0011 (DNA & Provenance)

## Summary

This RFC defines a **self-documenting code architecture** for GasHammer. A `cargo xtask docgen` command and a hive REST endpoint introspect the live codebase — parsing Rust source files with `syn`, extracting public API surfaces, doc comments, module hierarchies, and crate relationships — and dynamically generate an mdBook documentation site. The book is never manually maintained; it is always derived from code. If the code changes, one command regenerates the book. If a module has no doc comments, the book page is an empty skeleton that shames the author into writing them.

---

## 1. Design Principles

1. **Code is the single source of truth** — documentation is generated, not duplicated
2. **Empty pages are a feature** — a generated page with no content means the code has no docs; this is visible and measurable
3. **Zero manual SUMMARY.md maintenance** — the table of contents is derived from the crate/module tree
4. **Incremental generation** — only regenerate pages for changed files (content-hash based)
5. **Two access paths** — CLI for developers, REST for CI/CD and dashboards
6. **Provenance embedded** — every generated page carries GasHammer DNA (RFC-0011)

---

## 2. Architecture

```text
                                ┌────────────────────┐
                                │   Rust Source Code  │
                                │   crates/*/src/**   │
                                └────────┬───────────┘
                                         │
                            ┌────────────┴────────────┐
                            │     Source Walker        │
                            │  enumerate crates,       │
                            │  modules, source files   │
                            └────────────┬────────────┘
                                         │
                            ┌────────────┴────────────┐
                            │     syn Parser           │
                            │  extract: pub items,     │
                            │  doc comments, traits,   │
                            │  structs, enums, fns,    │
                            │  module hierarchy        │
                            └────────────┬────────────┘
                                         │
                            ┌────────────┴────────────┐
                            │     Semantic Model       │
                            │  CrateDoc, ModuleDoc,    │
                            │  ItemDoc — typed IR      │
                            └────────────┬────────────┘
                                         │
                       ┌─────────────────┼─────────────────┐
                       ▼                 ▼                 ▼
              ┌────────────────┐ ┌──────────────┐ ┌──────────────┐
              │ Page Generator │ │ SUMMARY.md   │ │ Metrics      │
              │ .md per module │ │ Generator    │ │ Collector    │
              └────────────────┘ └──────────────┘ └──────────────┘
                       │                 │                 │
                       ▼                 ▼                 ▼
              ┌────────────────────────────────────────────────────┐
              │                  docs/src/                         │
              │  SUMMARY.md + generated/**/*.md + coverage.json   │
              └────────────────────────────────────────────────────┘
                                         │
                                         ▼
                                    mdbook build
```

---

## 3. Source Walker

### 3.1 Discovery

The walker enumerates the workspace:

```rust
pub struct WorkspaceLayout {
    pub root: PathBuf,
    pub crates: Vec<CrateInfo>,
}

pub struct CrateInfo {
    pub name: String,              // e.g., "gashammer-nitro"
    pub path: PathBuf,             // e.g., "crates/gashammer-nitro"
    pub entry_point: PathBuf,      // e.g., "src/lib.rs"
    pub modules: Vec<ModuleInfo>,  // recursively discovered
    pub cargo_toml: CargoMetadata, // name, version, description, dependencies
}

pub struct ModuleInfo {
    pub name: String,
    pub file_path: PathBuf,
    pub is_dir_module: bool,       // mod.rs vs file.rs
    pub children: Vec<ModuleInfo>,
}
```

### 3.2 Discovery Rules

1. Read workspace `Cargo.toml` → enumerate `[workspace.members]`
2. For each crate, read its `Cargo.toml` for metadata (name, description, dependencies)
3. Parse `src/lib.rs` (or `src/main.rs`) as the entry point
4. Follow `mod` declarations to discover the full module tree
5. Handle both `mod foo;` (file-based) and `mod foo { ... }` (inline) modules
6. Ignore `#[cfg(test)]` modules and `tests/` directories by default (configurable)

---

## 4. syn Parser

### 4.1 Extraction Targets

For each source file, `syn` parses and extracts:

| Item | What's Extracted | Used For |
| --- | --- | --- |
| `//!` module docs | Full text | Chapter introduction |
| `/// ` doc comments | Full text | Item description |
| `pub struct` | Name, fields, generics, derives | Type reference |
| `pub enum` | Name, variants, generics | Type reference |
| `pub trait` | Name, methods, supertraits | API contract reference |
| `pub fn` | Name, signature, return type | Function reference |
| `pub type` | Name, target type | Type alias reference |
| `pub const` / `pub static` | Name, type, value | Constant reference |
| `impl Trait for Type` | Trait, type | Implementation map |
| `pub mod` | Name | Module tree |
| `pub use` | Re-exports | Cross-reference |
| `#[derive(...)]` | Derive list | Trait implementation notes |

### 4.2 Parser Implementation

```rust
use syn::{parse_file, Item, Visibility, Attribute};

pub struct FileAnalysis {
    pub module_doc: Option<String>,
    pub items: Vec<ItemDoc>,
}

pub struct ItemDoc {
    pub kind: ItemKind,
    pub name: String,
    pub doc_comment: Option<String>,
    pub signature: String,         // rendered Rust signature
    pub visibility: Visibility,
    pub attributes: Vec<String>,   // derive, cfg, etc.
    pub source_line: usize,
    pub source_file: PathBuf,
}

pub enum ItemKind {
    Struct { fields: Vec<FieldDoc> },
    Enum { variants: Vec<VariantDoc> },
    Trait { methods: Vec<MethodDoc>, supertraits: Vec<String> },
    Function { params: Vec<ParamDoc>, return_type: Option<String>, is_async: bool },
    TypeAlias { target: String },
    Constant { ty: String, value: Option<String> },
    Module,
    ReExport { path: String },
}
```

### 4.3 Doc Comment Extraction

```rust
fn extract_doc_comment(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if attr.path().is_ident("doc") {
                // Extract the string literal from #[doc = "..."]
                if let syn::Meta::NameValue(nv) = &attr.meta {
                    if let syn::Expr::Lit(lit) = &nv.value {
                        if let syn::Lit::Str(s) = &lit.lit {
                            return Some(s.value());
                        }
                    }
                }
            }
            None
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join("\n").trim().to_string())
    }
}
```

---

## 5. Semantic Model

The parser output is normalized into a typed intermediate representation:

```rust
pub struct BookModel {
    pub workspace_name: String,
    pub workspace_version: String,
    pub crates: Vec<CrateDoc>,
    pub generated_at: DateTime<Utc>,
    pub generator_version: String,
    pub coverage: CoverageReport,
}

pub struct CrateDoc {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub dependencies: Vec<String>,
    pub root_module: ModuleDoc,
}

pub struct ModuleDoc {
    pub name: String,
    pub full_path: String,         // e.g., "gashammer_nitro::rpc::provider"
    pub module_doc: Option<String>,
    pub items: Vec<ItemDoc>,
    pub children: Vec<ModuleDoc>,
    pub source_file: PathBuf,
    pub content_hash: String,      // SHA-256 of source file for incremental gen
}
```

This model is the single representation from which all outputs are derived.

---

## 6. Page Generator

### 6.1 Page Structure

Each module produces one mdBook page:

```markdown
# gashammer_nitro::rpc

> RPC client adapters for Arbitrum Nitro JSON-RPC endpoints.

*Source: [`crates/gashammer-nitro/src/rpc/mod.rs`](https://github.com/copyleftdev/gashammer/blob/main/crates/gashammer-nitro/src/rpc/mod.rs)*

---

## Types

### `RpcProvider`

> Connection-pooled JSON-RPC provider for Nitro endpoints.

```rust
pub struct RpcProvider {
    pub endpoint: Url,
    pub pool_size: usize,
    pub timeout: Duration,
}
```

**Derives:** `Debug`, `Clone`

---

### `RpcError`

> Errors from RPC interactions.

```rust
pub enum RpcError {
    Transport(TransportError),
    Timeout { endpoint: Url, waited: Duration },
    RateLimited { retry_after: Duration },
}
```

---

## Traits

### `NitroRpc`

> Trait abstracting Nitro-specific RPC operations.

```rust
pub trait NitroRpc: Send + Sync {
    async fn send_raw_transaction(&self, tx: Bytes) -> Result<TxHash, RpcError>;
    async fn get_transaction_receipt(&self, hash: TxHash) -> Result<Option<Receipt>, RpcError>;
}
```

---

## Functions

### `create_provider`

```rust
pub async fn create_provider(config: &RpcConfig) -> Result<RpcProvider, RpcError>
```

> Construct a new RPC provider with connection pooling.

---

*Generated by [GasHammer docgen](https://github.com/copyleftdev/gashammer) v0.1.0*
```

### 6.2 Empty Page Template

When a module has no doc comments, the page is generated with explicit empty markers:

```markdown
# gashammer_edge::pipeline

> **⚠ Undocumented module.** This page was auto-generated but the source
> contains no module-level documentation. Run `cargo xtask docgen --coverage`
> to see documentation coverage metrics.

*Source: [`crates/gashammer-edge/src/pipeline.rs`](https://github.com/copyleftdev/gashammer/blob/main/crates/gashammer-edge/src/pipeline.rs)*

---

## Types

### `Pipeline`

> *No documentation.*

```rust
pub struct Pipeline { /* fields omitted */ }
```

---

*Generated by [GasHammer docgen](https://github.com/copyleftdev/gashammer) v0.1.0*
```

This makes documentation gaps **visible and measurable** rather than invisible.

### 6.3 Page Rendering

```rust
pub struct PageRenderer {
    pub template_engine: tera::Tera,
    pub github_base_url: String,
    pub generator_version: String,
}

impl PageRenderer {
    pub fn render_module(&self, module: &ModuleDoc, crate_name: &str) -> String {
        let context = tera::Context::from_serialize(ModuleTemplateData {
            crate_name: crate_name.to_string(),
            module_path: module.full_path.clone(),
            module_doc: module.module_doc.clone(),
            source_link: self.source_link(&module.source_file),
            types: self.collect_types(module),
            traits: self.collect_traits(module),
            functions: self.collect_functions(module),
            constants: self.collect_constants(module),
            has_docs: module.module_doc.is_some(),
            generator_version: self.generator_version.clone(),
        }).unwrap();

        self.template_engine.render("module.md.tera", &context).unwrap()
    }
}
```

---

## 7. SUMMARY.md Generator

### 7.1 Structure

The generated `SUMMARY.md` follows this hierarchy:

```text
# Summary

[Introduction](./introduction.md)

---

# API Reference                          ← auto-generated section
- [gashammer-common](./api/common/mod.md)
  - [error](./api/common/error.md)
  - [config](./api/common/config.md)
- [gashammer-nitro](./api/nitro/mod.md)
  - [rpc](./api/nitro/rpc.md)
  - [feed](./api/nitro/feed.md)
  - [contracts](./api/nitro/contracts.md)
- [gashammer-edge](./api/edge/mod.md)
  - [pipeline](./api/edge/pipeline.md)
  - [rate_limiter](./api/edge/rate_limiter.md)
  - ...
- [gashammer-hive](./api/hive/mod.md)
  - ...

---

# Architecture                            ← hand-written section (preserved)
- [System Overview](./architecture/overview.md)
- ...
```

### 7.2 Preservation of Hand-Written Sections

The SUMMARY.md generator uses **section markers** to distinguish generated vs. hand-written content:

```markdown
<!-- gashammer:docgen:start -->
# API Reference
- [gashammer-common](./api/common/mod.md)
  ...
<!-- gashammer:docgen:end -->
```

Everything between the markers is regenerated. Everything outside is preserved. This allows the book to contain both auto-generated API reference and hand-written architecture/guide content.

---

## 8. Coverage Metrics

### 8.1 Coverage Report

The docgen produces a `coverage.json` artifact:

```json
{
  "generated_at": "2025-03-06T14:00:00Z",
  "workspace_coverage": {
    "modules_total": 42,
    "modules_documented": 31,
    "modules_undocumented": 11,
    "module_coverage_pct": 73.8,
    "items_total": 186,
    "items_documented": 124,
    "items_undocumented": 62,
    "item_coverage_pct": 66.7
  },
  "crates": [
    {
      "name": "gashammer-nitro",
      "module_coverage_pct": 90.0,
      "item_coverage_pct": 82.3,
      "undocumented_items": [
        "gashammer_nitro::feed::FeedReconnectPolicy",
        "gashammer_nitro::rpc::batch_request"
      ]
    }
  ]
}
```

### 8.2 CI Gate

Documentation coverage can be enforced in CI:

```bash
cargo xtask docgen --coverage --min-coverage 70
```

Exit code 1 if coverage is below the threshold. This ensures documentation doesn't rot.

### 8.3 Coverage Badge

Generate a badge-compatible JSON for shields.io:

```json
{
  "schemaVersion": 1,
  "label": "doc coverage",
  "message": "73.8%",
  "color": "yellow"
}
```

---

## 9. Incremental Generation

### 9.1 Content Hashing

Each generated page is keyed by the SHA-256 of its source file. A manifest tracks the last-generated hash:

```json
{
  "pages": {
    "api/nitro/rpc.md": {
      "source_hash": "a1b2c3...",
      "generated_at": "2025-03-06T14:00:00Z"
    }
  }
}
```

On re-run, only pages whose source hash changed are regenerated. This makes the tool fast enough for `--watch` mode.

### 9.2 Watch Mode

```bash
cargo xtask docgen --watch
```

Uses `notify` crate to watch `crates/*/src/**/*.rs` for changes. On change, re-parses the modified file and regenerates its page. Combined with `mdbook serve`, this gives live-reloading documentation during development.

---

## 10. CLI Interface

### 10.1 xtask Commands

```text
cargo xtask docgen
    Generate the full mdBook from source code.
    Output: docs/src/api/**/*.md + updated SUMMARY.md

cargo xtask docgen --watch
    Watch source files and regenerate on change.

cargo xtask docgen --coverage
    Print documentation coverage report to stdout.

cargo xtask docgen --coverage --min-coverage <N>
    Exit 1 if coverage is below N%.

cargo xtask docgen --coverage --json
    Output coverage.json for CI consumption.

cargo xtask docgen --clean
    Remove all generated pages (between markers).

cargo xtask docgen --dry-run
    Show what would be generated without writing files.

cargo xtask docgen --crate <name>
    Generate docs for a single crate only.
```

### 10.2 xtask Crate

```text
xtask/
├── Cargo.toml
└── src/
    ├── main.rs           # CLI entry point (clap)
    └── docgen/
        ├── mod.rs         # public interface
        ├── walker.rs      # workspace/crate/module discovery
        ├── parser.rs      # syn-based source file analysis
        ├── model.rs       # semantic IR (BookModel, CrateDoc, etc.)
        ├── renderer.rs    # tera-based page generation
        ├── summary.rs     # SUMMARY.md generation with marker preservation
        ├── coverage.rs    # coverage metrics computation
        └── incremental.rs # content-hash manifest for incremental gen
```

### 10.3 Dependencies

```toml
[package]
name = "xtask"
version = "0.1.0"
edition = "2021"

[dependencies]
syn = { version = "2", features = ["full", "parsing", "visit"] }
quote = "1"
proc-macro2 = "1"
walkdir = "2"
clap = { version = "4", features = ["derive"] }
tera = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
toml = "0.8"
sha2 = "0.10"
chrono = "0.4"
notify = { version = "6", optional = true }
colored = "2"

[features]
watch = ["notify"]
```

---

## 11. REST Endpoint

### 11.1 Hive API Integration

The hive exposes a documentation generation endpoint:

```text
POST /api/v1/docs/generate
    Trigger docgen on the hive's local checkout.
    Returns: 202 Accepted + job ID

GET /api/v1/docs/generate/{job_id}
    Poll job status.
    Returns: { "status": "running|done|failed", "coverage": {...} }

GET /api/v1/docs/coverage
    Return current coverage.json without regenerating.

GET /api/v1/docs/book
    Serve the built mdBook (static HTML).
```

### 11.2 Use Cases

| Consumer | Endpoint | Purpose |
| --- | --- | --- |
| **CI pipeline** | `POST /docs/generate` | Regenerate docs on every merge to main |
| **PR checks** | `GET /docs/coverage` | Enforce coverage threshold |
| **Dashboard** | `GET /docs/book` | Serve live documentation |
| **Developer** | `cargo xtask docgen` | Local generation during development |

### 11.3 Implementation

The REST handler invokes the same `docgen` library that the xtask uses:

```rust
// In gashammer-hive/src/api/docs.rs

use gashammer_docgen::{generate, GenerateOptions, CoverageReport};

pub async fn handle_generate(
    State(state): State<AppState>,
) -> impl IntoResponse {
    let job_id = Uuid::new_v4();

    // Spawn background task
    tokio::spawn(async move {
        let opts = GenerateOptions {
            workspace_root: state.workspace_root.clone(),
            output_dir: state.docs_output.clone(),
            incremental: true,
            ..Default::default()
        };

        match generate(&opts).await {
            Ok(report) => {
                state.doc_jobs.insert(job_id, JobResult::Done(report));
            }
            Err(e) => {
                state.doc_jobs.insert(job_id, JobResult::Failed(e.to_string()));
            }
        }
    });

    (StatusCode::ACCEPTED, Json(json!({ "job_id": job_id })))
}
```

### 11.4 Shared Library

To avoid duplicating logic between the xtask and the hive, the core docgen is a library crate:

```text
crates/
├── gashammer-docgen/        # library: parser, model, renderer
│   ├── Cargo.toml
│   ├── src/
│   │   ├── lib.rs
│   │   ├── walker.rs
│   │   ├── parser.rs
│   │   ├── model.rs
│   │   ├── renderer.rs
│   │   ├── summary.rs
│   │   ├── coverage.rs
│   │   └── incremental.rs
│   └── templates/
│       ├── module.md.tera
│       ├── crate_index.md.tera
│       └── summary.md.tera
└── ...

xtask/
├── Cargo.toml               # depends on gashammer-docgen
└── src/
    └── main.rs              # thin CLI wrapper
```

---

## 12. Template System

### 12.1 Module Page Template

```text
# {{ module_path }}

{% if module_doc %}
> {{ module_doc }}
{% else %}
> **⚠ Undocumented module.** Run `cargo xtask docgen --coverage` to see
> documentation coverage metrics.
{% endif %}

*Source: [`{{ source_path }}`]({{ source_link }})*

---

{% if types | length > 0 %}
## Types
{% for item in types %}

### `{{ item.name }}`

{% if item.doc %}
> {{ item.doc }}
{% else %}
> *No documentation.*
{% endif %}

```rust
{{ item.signature }}
```

{% if item.derives | length > 0 %}
**Derives:** {{ item.derives | join(sep=", ") }}
{% endif %}

{% endfor %}
{% endif %}

{% if traits | length > 0 %}
## Traits
{% for item in traits %}

### `{{ item.name }}`

{% if item.doc %}
> {{ item.doc }}
{% else %}
> *No documentation.*
{% endif %}

```rust
{{ item.signature }}
```

{% if item.supertraits | length > 0 %}
**Supertraits:** {{ item.supertraits | join(sep=" + ") }}
{% endif %}

{% endfor %}
{% endif %}

{% if functions | length > 0 %}
## Functions
{% for item in functions %}

### `{{ item.name }}`

```rust
{{ item.signature }}
```

{% if item.doc %}
> {{ item.doc }}
{% endif %}

{% endfor %}
{% endif %}

{% if constants | length > 0 %}
## Constants
{% for item in constants %}

### `{{ item.name }}`

```rust
{{ item.signature }}
```

{% if item.doc %}
> {{ item.doc }}
{% endif %}

{% endfor %}
{% endif %}

---

*Generated by [GasHammer docgen](https://github.com/copyleftdev/gashammer) v{{ generator_version }} — {{ generated_at }}*
```

### 12.2 Crate Index Template

Each crate gets an index page listing its modules:

```text
# {{ crate_name }}

> {{ crate_description | default(value="*No crate-level description.*") }}

**Version:** {{ crate_version }}
**Dependencies:** {{ dependencies | join(sep=", ") }}

## Modules

{% for module in modules %}
- [`{{ module.name }}`](./{{ module.name }}.md){% if module.has_docs %} — {{ module.summary }}{% endif %}
{% endfor %}

## Documentation Coverage

| Metric | Value |
| --- | --- |
| Modules documented | {{ coverage.modules_documented }}/{{ coverage.modules_total }} |
| Items documented | {{ coverage.items_documented }}/{{ coverage.items_total }} |
| Coverage | {{ coverage.pct }}% |
```

---

## 13. Integration with Existing Book Structure

### 13.1 Merged Book

The final mdBook combines three content sources:

```text
docs/src/
├── introduction.md              ← hand-written
├── getting-started/             ← hand-written
├── architecture/                ← hand-written
├── scenarios/                   ← hand-written
├── operations/                  ← hand-written
├── cli/                         ← hand-written
├── api/                         ← AUTO-GENERATED from code
│   ├── common/
│   │   ├── mod.md
│   │   ├── error.md
│   │   └── config.md
│   ├── nitro/
│   │   ├── mod.md
│   │   ├── rpc.md
│   │   ├── feed.md
│   │   └── contracts.md
│   ├── edge/
│   │   └── ...
│   └── hive/
│       └── ...
├── internals/                   ← hand-written
├── contributing/                ← hand-written
├── appendix/                    ← hand-written
├── SUMMARY.md                   ← partially generated (between markers)
└── .docgen-manifest.json        ← incremental build manifest
```

### 13.2 Build Pipeline

```bash
# Full docs build
cargo xtask docgen           # generates api/ pages + updates SUMMARY.md
mdbook build docs/           # builds static HTML site

# Development workflow
cargo xtask docgen --watch & # watches source, regenerates pages
mdbook serve docs/           # serves with live reload
```

---

## 14. CI Integration

### 14.1 GitHub Actions Workflow

```yaml
name: docs
on:
  push:
    branches: [main]
  pull_request:

jobs:
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install mdBook
        run: cargo install mdbook

      - name: Generate API docs
        run: cargo xtask docgen --coverage --min-coverage 70

      - name: Build book
        run: mdbook build docs/

      - name: Deploy to GitHub Pages
        if: github.ref == 'refs/heads/main'
        uses: peaceiris/actions-gh-pages@v3
        with:
          github_token: ${{ secrets.GITHUB_TOKEN }}
          publish_dir: docs/book
```

### 14.2 PR Coverage Comment

On pull requests, post a coverage diff comment:

```text
📚 Documentation Coverage

| Crate | Before | After | Δ |
| --- | --- | --- | --- |
| gashammer-nitro | 82.3% | 85.1% | +2.8% ✅ |
| gashammer-edge | 66.7% | 64.2% | -2.5% ⚠️ |
| **Total** | **73.8%** | **74.6%** | **+0.8%** |

New undocumented items:
- `gashammer_edge::pipeline::FlushPolicy`
- `gashammer_edge::pipeline::drain_buffer`
```

---

## 15. Risks

### Risk: syn parsing fails on macro-heavy code

Procedural macros expand at compile time; `syn` sees the unexpanded source.

**Mitigation:** The parser gracefully skips items it cannot parse. Macro-generated items (e.g., from `#[derive]`) are noted but not expanded. A `// docgen: skip` comment allows authors to exclude problematic items.

### Risk: Generated pages clutter git diffs

Auto-generated pages change frequently, creating noisy diffs.

**Mitigation:** Add `docs/src/api/` to `.gitattributes` as `linguist-generated=true` so GitHub collapses them in diffs. Alternatively, generate during CI only and never commit generated pages.

### Risk: tera template rendering is slow for large codebases

**Mitigation:** Templates are compiled once and reused. Incremental generation means only changed pages are re-rendered. Benchmarked expectation: <5 seconds for a 50-crate workspace.

### Risk: Module discovery misses conditional compilation

Items behind `#[cfg(feature = "...")]` may be included or excluded depending on active features.

**Mitigation:** Parser includes all items regardless of `cfg`. Items with `cfg` attributes are annotated in the output with their feature gate.

---

## 16. Open Questions

1. Should generated pages be committed to the repo or generated only in CI?
2. Should the docgen also extract and render `#[test]` function names as a "test index" per module?
3. Should the coverage report distinguish between public API docs and internal module docs?
4. Should the REST endpoint support generating docs for a specific git ref (branch/tag)?
5. Should the mdBook include a cross-crate dependency graph visualization (e.g., Mermaid diagram)?

---

## 17. References

- [mdBook](https://rust-lang.github.io/mdBook/)
- [`syn` crate](https://docs.rs/syn/latest/syn/)
- [`tera` template engine](https://keats.github.io/tera/)
- [cargo-xtask pattern](https://github.com/matklad/cargo-xtask)
- RFC-0002 §2 (Workspace Structure)
- RFC-0003 §6 (REST API)
- RFC-0011 §5 (Artifact DNA — report provenance footers)
