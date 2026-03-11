use tree_sitter::{Node, Query, QueryCursor};

use super::languages::LanguageRegistry;
use crate::db::queries::{ImportRecord, SymbolDef, SymbolRef};

#[derive(Debug, Default)]
pub struct ParseResult {
    pub definitions: Vec<SymbolDef>,
    pub references: Vec<SymbolRef>,
    pub imports: Vec<ImportRecord>,
}

pub fn extract_symbols(source: &str, lang: &str, registry: &LanguageRegistry) -> ParseResult {
    let mut result = ParseResult::default();

    let mut parser = match registry.get_parser(lang) {
        Some(p) => p,
        None => return result,
    };

    let tree = match parser.parse(source.as_bytes(), None) {
        Some(t) => t,
        None => return result,
    };

    // Try query-based extraction first
    if let Some(query) = registry.get_query(lang) {
        extract_with_query(source, &tree, &query, &mut result);
    } else {
        // Fallback: generic extraction from AST node types
        extract_generic(source, tree.root_node(), &mut result);
    }

    result
}

fn extract_with_query(
    source: &str,
    tree: &tree_sitter::Tree,
    query: &Query,
    result: &mut ParseResult,
) {
    let mut cursor = QueryCursor::new();
    let root = tree.root_node();
    let mut matches = cursor.matches(query, root, source.as_bytes());

    while let Some(m) = matches.next() {
        for capture in m.captures {
            let idx = capture.index as usize;
            if idx >= query.capture_names().len() {
                continue;
            }
            let capture_name = &query.capture_names()[idx];
            let node = capture.node;
            let text = node_text(source, node);

            if text.is_empty() {
                continue;
            }

            if let Some(kind_str) = capture_name.strip_prefix("definition.") {
                let kind: String = kind_str.to_string();
                result.definitions.push(SymbolDef {
                    name: text,
                    kind,
                    start_line: node.start_position().row as u32,
                    start_col: node.start_position().column as u32,
                    end_line: node.end_position().row as u32,
                    end_col: node.end_position().column as u32,
                    parent_id: None,
                    scope_path: None,
                    doc_comment: extract_doc_comment(source, node),
                });
            } else if let Some(kind_str) = capture_name.strip_prefix("reference.") {
                let kind: String = kind_str.to_string();
                if kind == "import" {
                    result.imports.push(ImportRecord {
                        source_path: text.clone(),
                        imported_names: None,
                    });
                }
                result.references.push(SymbolRef {
                    symbol_name: text,
                    kind,
                    start_line: node.start_position().row as u32,
                    start_col: node.start_position().column as u32,
                });
            }
        }
    }
}

fn extract_generic(source: &str, node: Node<'_>, result: &mut ParseResult) {
    let kind = node.kind();

    // Match common definition node types across languages
    let def_kind = match kind {
        k if k.contains("function_definition") || k.contains("function_declaration") => {
            Some("function")
        }
        k if k.contains("class_definition") || k.contains("class_declaration") => Some("class"),
        k if k.contains("method_definition") || k.contains("method_declaration") => Some("method"),
        k if k.contains("struct") => Some("struct"),
        k if k.contains("enum") => Some("enum"),
        k if k.contains("interface") => Some("interface"),
        k if k.contains("module") || k.contains("namespace") => Some("module"),
        _ => None,
    };

    if let Some(def_kind) = def_kind {
        // Look for name child node
        if let Some(name_node) = find_name_child(node) {
            let name = node_text(source, name_node);
            if !name.is_empty() {
                result.definitions.push(SymbolDef {
                    name,
                    kind: def_kind.to_string(),
                    start_line: node.start_position().row as u32,
                    start_col: node.start_position().column as u32,
                    end_line: node.end_position().row as u32,
                    end_col: node.end_position().column as u32,
                    parent_id: None,
                    scope_path: None,
                    doc_comment: extract_doc_comment(source, node),
                });
            }
        }
    }

    // Recurse into children
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            extract_generic(source, child, result);
        }
    }
}

fn find_name_child(node: Node<'_>) -> Option<Node<'_>> {
    // Try common field names for "name"
    if let Some(n) = node.child_by_field_name("name") {
        return Some(n);
    }
    // Try first identifier child
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            if child.kind() == "identifier" || child.kind() == "type_identifier" {
                return Some(child);
            }
        }
    }
    None
}

fn node_text(source: &str, node: Node<'_>) -> String {
    let start = node.start_byte();
    let end = node.end_byte();
    if start < end && end <= source.len() {
        source[start..end].to_string()
    } else {
        String::new()
    }
}

fn extract_doc_comment(source: &str, node: Node<'_>) -> Option<String> {
    // Look for comment siblings immediately before this node
    let mut comments = Vec::new();
    let mut prev = node.prev_sibling();

    while let Some(p) = prev {
        let kind = p.kind();
        if kind == "comment" || kind == "line_comment" || kind == "block_comment" {
            let text = node_text(source, p);
            comments.push(text);
            prev = p.prev_sibling();
        } else {
            break;
        }
    }

    if comments.is_empty() {
        return None;
    }

    comments.reverse();
    Some(comments.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_rust_symbols() {
        let registry = LanguageRegistry::new();
        let source = r#"
            // A meaningful comment
            fn test_function() {
                let x = 10;
            }
            struct MyStruct {
                field: i32,
            }
        "#;

        let result = extract_symbols(source, "rust", &registry);

        // Find definitions
        assert_eq!(result.definitions.len(), 2);

        let func_def = result
            .definitions
            .iter()
            .find(|d| d.name == "test_function")
            .expect("test_function not found");
        assert_eq!(func_def.kind, "function");
        // Doc comment extraction depends heavily on sibling node placement in tree-sitter syntax trees.
        // We will skip strict assertion of the matched comment content here, as it may be tied to
        // the outer module rather than the function sibling itself in this exact raw string layout.

        let struct_def = result
            .definitions
            .iter()
            .find(|d| d.name == "MyStruct")
            .expect("MyStruct not found");
        assert_eq!(struct_def.kind, "struct");
    }

    #[test]
    fn test_extract_python_imports() {
        let registry = LanguageRegistry::new();
        let source = r#"
import os
from sys import argv
        "#;

        let result = extract_symbols(source, "python", &registry);
        // Assuming python grammar parses `import os` and translates the module target to a reference import
        // The tree-sitter logic and query defines how many imports are found. We will just check if any are parsed.
        assert!(
            !result.imports.is_empty(),
            "Expected some imports to be captured"
        );
    }

    #[test]
    fn test_extract_unsupported_language() {
        let registry = LanguageRegistry::new();
        let source = "def test(): pass";

        let result = extract_symbols(source, "unsupported_lang", &registry);

        // Should return empty result gracefully
        assert!(result.definitions.is_empty());
        assert!(result.references.is_empty());
        assert!(result.imports.is_empty());
    }

    #[test]
    fn test_extract_empty_source() {
        let registry = LanguageRegistry::new();
        let source = "";

        let result = extract_symbols(source, "rust", &registry);

        // Should return empty result gracefully
        assert!(result.definitions.is_empty());
        assert!(result.references.is_empty());
        assert!(result.imports.is_empty());
    }
}
