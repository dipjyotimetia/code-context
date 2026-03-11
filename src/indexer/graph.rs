use tree_sitter::Node;

/// Build scope paths for symbols by analyzing AST nesting.
///
/// This provides light-weight relationship extraction: for each definition
/// we compute a dot-separated scope path (e.g. `MyClass.my_method`) by
/// walking up the tree and collecting enclosing definition names.
pub fn build_scope_paths(
    source: &str,
    root: Node<'_>,
    definitions: &mut [crate::db::queries::SymbolDef],
) {
    for def in definitions.iter_mut() {
        let start_byte = line_col_to_byte(source, def.start_line, def.start_col);
        if let Some(node) = find_node_at(root, start_byte) {
            def.scope_path = Some(compute_scope_path(source, node));
        }
    }
}

fn compute_scope_path(source: &str, node: Node<'_>) -> String {
    let mut parts = Vec::new();
    let mut current = node.parent();

    while let Some(parent) = current {
        let kind = parent.kind();
        let is_scope = kind.contains("function")
            || kind.contains("class")
            || kind.contains("struct")
            || kind.contains("impl")
            || kind.contains("module")
            || kind.contains("namespace")
            || kind.contains("trait")
            || kind.contains("interface")
            || kind.contains("method")
            || kind.contains("object");

        if is_scope && let Some(name_node) = parent.child_by_field_name("name") {
            let name = node_text(source, name_node);
            if !name.is_empty() {
                parts.push(name);
            }
        }

        current = parent.parent();
    }

    parts.reverse();
    parts.join(".")
}

fn find_node_at(root: Node<'_>, byte_offset: usize) -> Option<Node<'_>> {
    let mut node = root;
    loop {
        let mut found_child = false;
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i)
                && child.start_byte() <= byte_offset
                && byte_offset < child.end_byte()
            {
                node = child;
                found_child = true;
                break;
            }
        }
        if !found_child {
            break;
        }
    }
    if node.start_byte() <= byte_offset && byte_offset <= node.end_byte() {
        Some(node)
    } else {
        None
    }
}

fn line_col_to_byte(source: &str, line: u32, col: u32) -> usize {
    let mut byte = 0;
    let target_line = line as usize;
    for (i, raw_line) in source.split('\n').enumerate() {
        if i == target_line {
            // Handle CRLF: strip trailing \r for column math
            let clean_len = raw_line
                .strip_suffix('\r')
                .map_or(raw_line.len(), |s| s.len());
            return byte + (col as usize).min(clean_len);
        }
        byte += raw_line.len() + 1; // +1 for the '\n' we split on
    }
    byte
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
