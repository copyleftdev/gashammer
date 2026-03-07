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

//! Semantic model for documentation generation.
//!
//! Intermediate representation (IR) between source parsing and page generation.
//! The model captures public API items, doc comments, and coverage metrics.
//!
//! Ref: RFC-0012 §4.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Book model
// ---------------------------------------------------------------------------

/// Complete documentation model for all workspace crates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookModel {
    /// Documentation for each crate.
    pub crates: Vec<CrateDoc>,
    /// Coverage metrics.
    pub coverage: CoverageMetrics,
    /// Generator version (DNA).
    pub generator_version: String,
}

impl BookModel {
    /// Create a new book model from crate docs.
    pub fn from_crates(crates: Vec<CrateDoc>) -> Self {
        let coverage = CoverageMetrics::compute(&crates);
        Self {
            crates,
            coverage,
            generator_version: format!("gashammer-docgen {}", env!("CARGO_PKG_VERSION")),
        }
    }
}

// ---------------------------------------------------------------------------
// Crate documentation
// ---------------------------------------------------------------------------

/// Documentation for a single workspace crate.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateDoc {
    /// Crate name (e.g., "gashammer-common").
    pub name: String,
    /// Crate description from Cargo.toml.
    pub description: String,
    /// Module-level doc comment from lib.rs.
    pub doc_comment: String,
    /// Modules within this crate.
    pub modules: Vec<ModuleDoc>,
}

// ---------------------------------------------------------------------------
// Module documentation
// ---------------------------------------------------------------------------

/// Documentation for a single module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleDoc {
    /// Module name (e.g., "error").
    pub name: String,
    /// Full module path (e.g., "gashammer_common::error").
    pub path: String,
    /// Source file path relative to workspace root.
    pub source_file: String,
    /// Module-level doc comment.
    pub doc_comment: String,
    /// Public items in this module.
    pub items: Vec<ItemDoc>,
}

impl ModuleDoc {
    /// Whether this module has documentation.
    pub fn is_documented(&self) -> bool {
        !self.doc_comment.is_empty()
    }

    /// Count of public items with doc comments.
    pub fn documented_items(&self) -> usize {
        self.items.iter().filter(|i| i.is_documented()).count()
    }
}

// ---------------------------------------------------------------------------
// Item documentation
// ---------------------------------------------------------------------------

/// Documentation for a single public item.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ItemDoc {
    /// Item name.
    pub name: String,
    /// Kind of item.
    pub kind: ItemKind,
    /// Doc comment.
    pub doc_comment: String,
    /// Derive macros (for structs/enums).
    pub derives: Vec<String>,
    /// Signature (simplified).
    pub signature: String,
    /// Line number in source file.
    pub line_number: usize,
}

impl ItemDoc {
    /// Whether this item has documentation.
    pub fn is_documented(&self) -> bool {
        !self.doc_comment.is_empty()
    }
}

/// Kind of a public API item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ItemKind {
    Struct,
    Enum,
    Trait,
    Function,
    TypeAlias,
    Constant,
    Impl,
}

impl std::fmt::Display for ItemKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Struct => write!(f, "struct"),
            Self::Enum => write!(f, "enum"),
            Self::Trait => write!(f, "trait"),
            Self::Function => write!(f, "fn"),
            Self::TypeAlias => write!(f, "type"),
            Self::Constant => write!(f, "const"),
            Self::Impl => write!(f, "impl"),
        }
    }
}

// ---------------------------------------------------------------------------
// Coverage metrics
// ---------------------------------------------------------------------------

/// Documentation coverage metrics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageMetrics {
    pub crates_total: usize,
    pub modules_total: usize,
    pub modules_documented: usize,
    pub items_total: usize,
    pub items_documented: usize,
    pub coverage_pct: f64,
}

impl CoverageMetrics {
    /// Compute coverage from a set of crate docs.
    pub fn compute(crates: &[CrateDoc]) -> Self {
        let crates_total = crates.len();
        let modules_total: usize = crates.iter().map(|c| c.modules.len()).sum();
        let modules_documented: usize = crates
            .iter()
            .flat_map(|c| c.modules.iter())
            .filter(|m| m.is_documented())
            .count();
        let items_total: usize = crates
            .iter()
            .flat_map(|c| c.modules.iter())
            .map(|m| m.items.len())
            .sum();
        let items_documented: usize = crates
            .iter()
            .flat_map(|c| c.modules.iter())
            .map(|m| m.documented_items())
            .sum();
        let coverage_pct = if items_total > 0 {
            (items_documented as f64 / items_total as f64) * 100.0
        } else {
            100.0
        };

        Self {
            crates_total,
            modules_total,
            modules_documented,
            items_total,
            items_documented,
            coverage_pct,
        }
    }

    /// Check if coverage meets the minimum threshold.
    pub fn meets_threshold(&self, min_pct: f64) -> bool {
        self.coverage_pct >= min_pct
    }
}

// ---------------------------------------------------------------------------
// Page generation
// ---------------------------------------------------------------------------

/// Generate a Markdown page for a module.
pub fn generate_module_page(module: &ModuleDoc, repo_url: &str) -> String {
    let mut md = String::with_capacity(2048);

    // Title
    md.push_str(&format!("# `{}`\n\n", module.path));

    // Source link
    md.push_str(&format!(
        "**Source:** [`{}`]({}/blob/main/{})\n\n",
        module.source_file, repo_url, module.source_file
    ));

    // Module doc
    if module.is_documented() {
        md.push_str(&module.doc_comment);
        md.push_str("\n\n");
    } else {
        md.push_str("⚠ **Undocumented module** — this module needs documentation.\n\n");
    }

    // Group items by kind
    let structs: Vec<_> = module
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Struct)
        .collect();
    let enums: Vec<_> = module
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Enum)
        .collect();
    let traits: Vec<_> = module
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Trait)
        .collect();
    let functions: Vec<_> = module
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Function)
        .collect();
    let constants: Vec<_> = module
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::Constant)
        .collect();
    let type_aliases: Vec<_> = module
        .items
        .iter()
        .filter(|i| i.kind == ItemKind::TypeAlias)
        .collect();

    if !structs.is_empty() {
        md.push_str("## Structs\n\n");
        for item in &structs {
            render_item(&mut md, item, &module.source_file, repo_url);
        }
    }

    if !enums.is_empty() {
        md.push_str("## Enums\n\n");
        for item in &enums {
            render_item(&mut md, item, &module.source_file, repo_url);
        }
    }

    if !traits.is_empty() {
        md.push_str("## Traits\n\n");
        for item in &traits {
            render_item(&mut md, item, &module.source_file, repo_url);
        }
    }

    if !functions.is_empty() {
        md.push_str("## Functions\n\n");
        for item in &functions {
            render_item(&mut md, item, &module.source_file, repo_url);
        }
    }

    if !constants.is_empty() {
        md.push_str("## Constants\n\n");
        for item in &constants {
            render_item(&mut md, item, &module.source_file, repo_url);
        }
    }

    if !type_aliases.is_empty() {
        md.push_str("## Type Aliases\n\n");
        for item in &type_aliases {
            render_item(&mut md, item, &module.source_file, repo_url);
        }
    }

    // Footer (DNA)
    md.push_str("---\n\n");
    md.push_str(&format!(
        "*Generated by GasHammer docgen v{}*\n",
        env!("CARGO_PKG_VERSION")
    ));

    md
}

/// Render a single item as Markdown.
fn render_item(md: &mut String, item: &ItemDoc, source_file: &str, repo_url: &str) {
    md.push_str(&format!("### `{}`\n\n", item.name));

    // Signature
    md.push_str(&format!("```rust\n{}\n```\n\n", item.signature));

    // Source link
    md.push_str(&format!(
        "[source]({}/blob/main/{}#L{})\n\n",
        repo_url, source_file, item.line_number
    ));

    // Doc comment
    if item.is_documented() {
        md.push_str(&item.doc_comment);
        md.push_str("\n\n");
    }

    // Derives
    if !item.derives.is_empty() {
        md.push_str(&format!("**Derives:** {}\n\n", item.derives.join(", ")));
    }
}

/// Generate a SUMMARY.md section for a crate.
pub fn generate_summary_entry(crate_doc: &CrateDoc) -> String {
    let mut lines = Vec::new();
    let crate_slug = crate_doc.name.replace('-', "_");
    lines.push(format!(
        "- [{}](api/{}/index.md)",
        crate_doc.name, crate_slug
    ));
    for module in &crate_doc.modules {
        let module_slug = module.name.replace("::", "/");
        lines.push(format!(
            "  - [{}](api/{}/{}.md)",
            module.name, crate_slug, module_slug
        ));
    }
    lines.join("\n")
}

/// Marker for the start of auto-generated SUMMARY.md content.
pub const SUMMARY_MARKER_START: &str = "<!-- gashammer:docgen:start -->";
/// Marker for the end of auto-generated SUMMARY.md content.
pub const SUMMARY_MARKER_END: &str = "<!-- gashammer:docgen:end -->";

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_item(name: &str, kind: ItemKind, documented: bool) -> ItemDoc {
        let signature = format!("pub {kind} {name}");
        ItemDoc {
            name: name.into(),
            kind,
            doc_comment: if documented {
                "Documentation here.".into()
            } else {
                String::new()
            },
            derives: vec![],
            signature,
            line_number: 1,
        }
    }

    fn sample_module(name: &str, documented: bool, items: Vec<ItemDoc>) -> ModuleDoc {
        ModuleDoc {
            name: name.into(),
            path: format!("gashammer_common::{name}"),
            source_file: format!("crates/gashammer-common/src/{name}.rs"),
            doc_comment: if documented {
                "Module docs.".into()
            } else {
                String::new()
            },
            items,
        }
    }

    fn sample_crate() -> CrateDoc {
        CrateDoc {
            name: "gashammer-common".into(),
            description: "Shared types".into(),
            doc_comment: "Common crate.".into(),
            modules: vec![
                sample_module(
                    "error",
                    true,
                    vec![
                        sample_item("CommonError", ItemKind::Enum, true),
                        sample_item("ErrorCode", ItemKind::Enum, false),
                    ],
                ),
                sample_module("types", false, vec![]),
            ],
        }
    }

    #[test]
    fn test_item_kind_display() {
        assert_eq!(ItemKind::Struct.to_string(), "struct");
        assert_eq!(ItemKind::Enum.to_string(), "enum");
        assert_eq!(ItemKind::Function.to_string(), "fn");
    }

    #[test]
    fn test_item_is_documented() {
        let documented = sample_item("Foo", ItemKind::Struct, true);
        let undocumented = sample_item("Bar", ItemKind::Struct, false);
        assert!(documented.is_documented());
        assert!(!undocumented.is_documented());
    }

    #[test]
    fn test_module_is_documented() {
        let documented = sample_module("error", true, vec![]);
        let undocumented = sample_module("types", false, vec![]);
        assert!(documented.is_documented());
        assert!(!undocumented.is_documented());
    }

    #[test]
    fn test_module_documented_items() {
        let module = sample_module(
            "error",
            true,
            vec![
                sample_item("A", ItemKind::Struct, true),
                sample_item("B", ItemKind::Struct, false),
                sample_item("C", ItemKind::Function, true),
            ],
        );
        assert_eq!(module.documented_items(), 2);
    }

    #[test]
    fn test_coverage_metrics_compute() {
        let crates = vec![sample_crate()];
        let coverage = CoverageMetrics::compute(&crates);
        assert_eq!(coverage.crates_total, 1);
        assert_eq!(coverage.modules_total, 2);
        assert_eq!(coverage.modules_documented, 1);
        assert_eq!(coverage.items_total, 2);
        assert_eq!(coverage.items_documented, 1);
        assert!((coverage.coverage_pct - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_coverage_meets_threshold() {
        let metrics = CoverageMetrics {
            crates_total: 1,
            modules_total: 1,
            modules_documented: 1,
            items_total: 10,
            items_documented: 8,
            coverage_pct: 80.0,
        };
        assert!(metrics.meets_threshold(70.0));
        assert!(metrics.meets_threshold(80.0));
        assert!(!metrics.meets_threshold(81.0));
    }

    #[test]
    fn test_coverage_empty() {
        let coverage = CoverageMetrics::compute(&[]);
        assert_eq!(coverage.crates_total, 0);
        assert_eq!(coverage.items_total, 0);
        assert!((coverage.coverage_pct - 100.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_book_model_from_crates() {
        let model = BookModel::from_crates(vec![sample_crate()]);
        assert_eq!(model.crates.len(), 1);
        assert!(model.generator_version.contains("gashammer-docgen"));
    }

    #[test]
    fn test_generate_module_page_documented() {
        let module = sample_module(
            "error",
            true,
            vec![sample_item("CommonError", ItemKind::Enum, true)],
        );
        let page = generate_module_page(&module, "https://github.com/copyleftdev/gashammer");
        assert!(page.contains("# `gashammer_common::error`"));
        assert!(page.contains("Module docs."));
        assert!(page.contains("## Enums"));
        assert!(page.contains("### `CommonError`"));
        assert!(page.contains("Generated by GasHammer docgen"));
    }

    #[test]
    fn test_generate_module_page_undocumented() {
        let module = sample_module("types", false, vec![]);
        let page = generate_module_page(&module, "https://github.com/copyleftdev/gashammer");
        assert!(page.contains("⚠ **Undocumented module**"));
    }

    #[test]
    fn test_generate_module_page_source_link() {
        let module = sample_module("error", true, vec![]);
        let page = generate_module_page(&module, "https://github.com/copyleftdev/gashammer");
        assert!(page.contains("blob/main/crates/gashammer-common/src/error.rs"));
    }

    #[test]
    fn test_generate_summary_entry() {
        let crate_doc = sample_crate();
        let summary = generate_summary_entry(&crate_doc);
        assert!(summary.contains("gashammer-common"));
        assert!(summary.contains("api/gashammer_common/error.md"));
        assert!(summary.contains("api/gashammer_common/types.md"));
    }

    #[test]
    fn test_model_serialization() {
        let model = BookModel::from_crates(vec![sample_crate()]);
        let json = serde_json::to_string(&model).expect("serialize");
        let parsed: BookModel = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.crates.len(), 1);
        assert_eq!(parsed.coverage.items_total, 2);
    }

    #[test]
    fn test_coverage_serialization() {
        let metrics = CoverageMetrics {
            crates_total: 12,
            modules_total: 36,
            modules_documented: 30,
            items_total: 200,
            items_documented: 150,
            coverage_pct: 75.0,
        };
        let json = serde_json::to_string(&metrics).expect("serialize");
        let parsed: CoverageMetrics = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed.items_total, 200);
        assert!((parsed.coverage_pct - 75.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_summary_markers() {
        assert!(SUMMARY_MARKER_START.contains("docgen:start"));
        assert!(SUMMARY_MARKER_END.contains("docgen:end"));
    }

    #[test]
    fn test_render_item_with_derives() {
        let item = ItemDoc {
            name: "RunId".into(),
            kind: ItemKind::Struct,
            doc_comment: "A run identifier.".into(),
            derives: vec!["Debug".into(), "Clone".into(), "Serialize".into()],
            signature: "pub struct RunId(pub Uuid)".into(),
            line_number: 42,
        };
        let mut md = String::new();
        render_item(
            &mut md,
            &item,
            "crates/gashammer-common/src/types.rs",
            "https://github.com/copyleftdev/gashammer",
        );
        assert!(md.contains("### `RunId`"));
        assert!(md.contains("pub struct RunId(pub Uuid)"));
        assert!(md.contains("#L42"));
        assert!(md.contains("Debug, Clone, Serialize"));
    }
}
