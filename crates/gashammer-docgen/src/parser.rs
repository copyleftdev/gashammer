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
// GasHammer is a registered trademark of Don Johnson.

//! Source parser — extract public API items from Rust source files using `syn`.
//!
//! Parses `.rs` files and builds `ModuleDoc` / `ItemDoc` structures for
//! the documentation model.
//!
//! Ref: RFC-0012 §3.

use syn::{Attribute, Item, Visibility};

use crate::model::{ItemDoc, ItemKind, ModuleDoc};

// ---------------------------------------------------------------------------
// Extract doc comments from attributes
// ---------------------------------------------------------------------------

/// Extract doc comment text from a list of attributes.
pub fn extract_doc_comment(attrs: &[Attribute]) -> String {
    let lines: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    return Some(s.value());
                }
            }
            None
        })
        .collect();

    // Trim leading space from each line (standard doc comment formatting)
    lines
        .iter()
        .map(|l| l.strip_prefix(' ').unwrap_or(l).to_string())
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

/// Extract derive macro names from attributes.
pub fn extract_derives(attrs: &[Attribute]) -> Vec<String> {
    let mut derives = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("derive") {
            if let Ok(nested) = attr.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) {
                for path in nested {
                    if let Some(ident) = path.get_ident() {
                        derives.push(ident.to_string());
                    } else if let Some(seg) = path.segments.last() {
                        derives.push(seg.ident.to_string());
                    }
                }
            }
        }
    }
    derives
}

/// Check if a visibility is public (pub or pub(crate) etc).
fn is_pub(vis: &Visibility) -> bool {
    matches!(vis, Visibility::Public(_))
}

// ---------------------------------------------------------------------------
// Parse a source file into items
// ---------------------------------------------------------------------------

/// Parse a Rust source file and extract public items.
pub fn parse_source(
    source: &str,
    module_name: &str,
    module_path: &str,
    source_file: &str,
) -> ModuleDoc {
    let file = match syn::parse_file(source) {
        Ok(f) => f,
        Err(_) => {
            return ModuleDoc {
                name: module_name.into(),
                path: module_path.into(),
                source_file: source_file.into(),
                doc_comment: String::new(),
                items: vec![],
            };
        }
    };

    // Module-level doc comment (inner doc comments: //!)
    let module_doc = extract_doc_comment(&file.attrs);

    let mut items = Vec::new();

    for item in &file.items {
        if let Some(doc_item) = extract_item(item) {
            items.push(doc_item);
        }
    }

    ModuleDoc {
        name: module_name.into(),
        path: module_path.into(),
        source_file: source_file.into(),
        doc_comment: module_doc,
        items,
    }
}

/// Extract a public item from a syn Item.
fn extract_item(item: &Item) -> Option<ItemDoc> {
    match item {
        Item::Struct(s) if is_pub(&s.vis) => Some(ItemDoc {
            name: s.ident.to_string(),
            kind: ItemKind::Struct,
            doc_comment: extract_doc_comment(&s.attrs),
            derives: extract_derives(&s.attrs),
            signature: format_struct_sig(s),
            line_number: span_line(&s.ident),
        }),
        Item::Enum(e) if is_pub(&e.vis) => Some(ItemDoc {
            name: e.ident.to_string(),
            kind: ItemKind::Enum,
            doc_comment: extract_doc_comment(&e.attrs),
            derives: extract_derives(&e.attrs),
            signature: format_enum_sig(e),
            line_number: span_line(&e.ident),
        }),
        Item::Trait(t) if is_pub(&t.vis) => Some(ItemDoc {
            name: t.ident.to_string(),
            kind: ItemKind::Trait,
            doc_comment: extract_doc_comment(&t.attrs),
            derives: vec![],
            signature: format!("pub trait {}", t.ident),
            line_number: span_line(&t.ident),
        }),
        Item::Fn(f) if is_pub(&f.vis) => Some(ItemDoc {
            name: f.sig.ident.to_string(),
            kind: ItemKind::Function,
            doc_comment: extract_doc_comment(&f.attrs),
            derives: vec![],
            signature: format_fn_sig(&f.sig),
            line_number: span_line(&f.sig.ident),
        }),
        Item::Type(t) if is_pub(&t.vis) => Some(ItemDoc {
            name: t.ident.to_string(),
            kind: ItemKind::TypeAlias,
            doc_comment: extract_doc_comment(&t.attrs),
            derives: vec![],
            signature: format!("pub type {}", t.ident),
            line_number: span_line(&t.ident),
        }),
        Item::Const(c) if is_pub(&c.vis) => Some(ItemDoc {
            name: c.ident.to_string(),
            kind: ItemKind::Constant,
            doc_comment: extract_doc_comment(&c.attrs),
            derives: vec![],
            signature: format!("pub const {}", c.ident),
            line_number: span_line(&c.ident),
        }),
        _ => None,
    }
}

/// Get line number from a span.
///
/// Without the `span-locations` feature on proc-macro2, we return 0.
fn span_line(_ident: &syn::Ident) -> usize {
    // proc_macro2::Span::start() requires the `span-locations` feature.
    // We intentionally omit it to keep compile times fast. Line numbers
    // are a nice-to-have for source links; 0 means "unknown".
    0
}

/// Format a struct signature.
fn format_struct_sig(s: &syn::ItemStruct) -> String {
    let generics = if s.generics.params.is_empty() {
        String::new()
    } else {
        "<...>".to_string()
    };
    match &s.fields {
        syn::Fields::Named(_) => format!("pub struct {}{} {{ ... }}", s.ident, generics),
        syn::Fields::Unnamed(_) => format!("pub struct {}{}(...)", s.ident, generics),
        syn::Fields::Unit => format!("pub struct {}{}", s.ident, generics),
    }
}

/// Format an enum signature.
fn format_enum_sig(e: &syn::ItemEnum) -> String {
    let variants: Vec<String> = e.variants.iter().map(|v| v.ident.to_string()).collect();
    let variants_str = if variants.len() <= 5 {
        variants.join(", ")
    } else {
        format!(
            "{}, ... ({} total)",
            variants[..3].join(", "),
            variants.len()
        )
    };
    format!("pub enum {} {{ {} }}", e.ident, variants_str)
}

/// Format a function signature.
fn format_fn_sig(sig: &syn::Signature) -> String {
    let asyncness = if sig.asyncness.is_some() {
        "async "
    } else {
        ""
    };
    let params: Vec<String> = sig
        .inputs
        .iter()
        .map(|arg| match arg {
            syn::FnArg::Receiver(r) => {
                if r.reference.is_some() {
                    if r.mutability.is_some() {
                        "&mut self".into()
                    } else {
                        "&self".into()
                    }
                } else {
                    "self".into()
                }
            }
            syn::FnArg::Typed(t) => {
                if let syn::Pat::Ident(pi) = &*t.pat {
                    pi.ident.to_string()
                } else {
                    "_".into()
                }
            }
        })
        .collect();

    let ret = match &sig.output {
        syn::ReturnType::Default => String::new(),
        syn::ReturnType::Type(_, _) => " -> ...".into(),
    };

    format!(
        "pub {}fn {}({}){}",
        asyncness,
        sig.ident,
        params.join(", "),
        ret
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_SOURCE: &str = r#"
//! Module documentation here.

use std::collections::HashMap;

/// A documented struct.
#[derive(Debug, Clone, Serialize)]
pub struct MyStruct {
    pub field: String,
}

/// A documented enum.
#[derive(Debug)]
pub enum MyEnum {
    A,
    B,
    C,
}

/// A public function.
pub fn my_function(x: u32) -> String {
    x.to_string()
}

/// A constant.
pub const MAX_VALUE: u64 = 1000;

// Private items should not be extracted
struct PrivateStruct;
fn private_fn() {}
"#;

    #[test]
    fn test_parse_source_extracts_module_doc() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        assert_eq!(module.doc_comment, "Module documentation here.");
    }

    #[test]
    fn test_parse_source_extracts_struct() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        let structs: Vec<_> = module
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Struct)
            .collect();
        assert_eq!(structs.len(), 1);
        assert_eq!(structs[0].name, "MyStruct");
        assert!(structs[0].doc_comment.contains("documented struct"));
    }

    #[test]
    fn test_parse_source_extracts_enum() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        let enums: Vec<_> = module
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Enum)
            .collect();
        assert_eq!(enums.len(), 1);
        assert_eq!(enums[0].name, "MyEnum");
        assert!(enums[0].signature.contains("A, B, C"));
    }

    #[test]
    fn test_parse_source_extracts_function() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        let fns: Vec<_> = module
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Function)
            .collect();
        assert_eq!(fns.len(), 1);
        assert_eq!(fns[0].name, "my_function");
    }

    #[test]
    fn test_parse_source_extracts_constant() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        let consts: Vec<_> = module
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Constant)
            .collect();
        assert_eq!(consts.len(), 1);
        assert_eq!(consts[0].name, "MAX_VALUE");
    }

    #[test]
    fn test_parse_source_skips_private() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        let names: Vec<&str> = module.items.iter().map(|i| i.name.as_str()).collect();
        assert!(!names.contains(&"PrivateStruct"));
        assert!(!names.contains(&"private_fn"));
    }

    #[test]
    fn test_extract_doc_comment() {
        let source = r#"
/// First line.
/// Second line.
pub fn foo() {}
"#;
        let file = syn::parse_file(source).unwrap();
        if let Item::Fn(f) = &file.items[0] {
            let doc = extract_doc_comment(&f.attrs);
            assert!(doc.contains("First line."));
            assert!(doc.contains("Second line."));
        }
    }

    #[test]
    fn test_extract_derives() {
        let source = r#"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Foo;
"#;
        let file = syn::parse_file(source).unwrap();
        if let Item::Struct(s) = &file.items[0] {
            let derives = extract_derives(&s.attrs);
            assert_eq!(derives, vec!["Debug", "Clone", "Serialize", "Deserialize"]);
        }
    }

    #[test]
    fn test_parse_invalid_source() {
        let module = parse_source("{{invalid rust}}", "bad", "crate::bad", "src/bad.rs");
        assert!(module.items.is_empty());
        assert!(module.doc_comment.is_empty());
    }

    #[test]
    fn test_parse_trait() {
        let source = r#"
/// A trait.
pub trait MyTrait {
    fn do_thing(&self);
}
"#;
        let module = parse_source(source, "test", "crate::test", "src/test.rs");
        let traits: Vec<_> = module
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::Trait)
            .collect();
        assert_eq!(traits.len(), 1);
        assert_eq!(traits[0].name, "MyTrait");
    }

    #[test]
    fn test_parse_type_alias() {
        let source = r#"
/// A type alias.
pub type Result<T> = std::result::Result<T, MyError>;
"#;
        let module = parse_source(source, "test", "crate::test", "src/test.rs");
        let types: Vec<_> = module
            .items
            .iter()
            .filter(|i| i.kind == ItemKind::TypeAlias)
            .collect();
        assert_eq!(types.len(), 1);
        assert_eq!(types[0].name, "Result");
    }

    #[test]
    fn test_format_fn_sig_async() {
        let source = r#"
pub async fn do_async(x: u32) -> String { String::new() }
"#;
        let file = syn::parse_file(source).unwrap();
        if let Item::Fn(f) = &file.items[0] {
            let sig = format_fn_sig(&f.sig);
            assert!(sig.contains("async"));
            assert!(sig.contains("do_async"));
        }
    }

    #[test]
    fn test_item_count() {
        let module = parse_source(SAMPLE_SOURCE, "test", "crate::test", "src/test.rs");
        // Should have: MyStruct, MyEnum, my_function, MAX_VALUE = 4 items
        assert_eq!(module.items.len(), 4);
    }
}
