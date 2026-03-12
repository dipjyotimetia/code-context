use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::Arc;

use rmcp::model::CallToolResult;
use rusqlite::params_from_iter;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::db::queries;
use crate::state::AppState;

fn normalize_import_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn import_candidates(file_path: &str) -> Vec<String> {
    let mut candidates = BTreeSet::new();
    let normalized = normalize_import_path(file_path);
    let trimmed = normalized.strip_prefix("./").unwrap_or(&normalized);
    candidates.insert(trimmed.to_string());

    let path = Path::new(trimmed);
    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
        candidates.insert(file_name.to_string());
    }

    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
        candidates.insert(stem.to_string());
        if let Some(parent) = path.parent().and_then(|p| p.to_str()) {
            let parent = parent.trim_end_matches('/');
            if !parent.is_empty() {
                candidates.insert(format!("{parent}/{stem}"));
            }
        }
    }

    if let Some(stripped) = trimmed.strip_suffix("/mod.rs") {
        candidates.insert(stripped.to_string());
        if let Some(mod_name) = stripped.rsplit('/').next()
            && !mod_name.is_empty()
        {
            candidates.insert(mod_name.to_string());
        }
    }

    candidates.into_iter().collect()
}

fn importers_for_path(
    conn: &rusqlite::Connection,
    file_path: &str,
) -> rusqlite::Result<Vec<String>> {
    let exact = conn
        .prepare(
            "SELECT DISTINCT f.path FROM imports i
             JOIN files f ON i.file_id = f.id
             WHERE i.source_path = ?1",
        )?
        .query_map(rusqlite::params![file_path], |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    if !exact.is_empty() {
        return Ok(exact);
    }

    let candidates = import_candidates(file_path);
    if candidates.is_empty() {
        return Ok(vec![]);
    }

    let placeholders = std::iter::repeat_n("?", candidates.len())
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "SELECT DISTINCT f.path FROM imports i
         JOIN files f ON i.file_id = f.id
         WHERE i.source_path IN ({placeholders})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt
        .query_map(params_from_iter(candidates.iter()), |row| row.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(rows)
}

// ── Call graph tool ─────────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetCallGraphArgs {
    #[schemars(description = "Symbol name to build a call graph for")]
    pub symbol: String,
    #[schemars(description = "Maximum depth to traverse (default: 2, max: 5)")]
    pub depth: Option<u32>,
}

pub async fn get_call_graph(
    state: &AppState,
    args: GetCallGraphArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let db = Arc::clone(&state.db);
    let symbol = args.symbol.clone();
    let max_depth = args.depth.unwrap_or(2).min(5);

    let graph = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let mut edges: Vec<(String, String, String)> = Vec::new(); // (caller_file, caller, callee)
            let mut visited: BTreeSet<String> = BTreeSet::new();
            let mut queue: VecDeque<(String, u32)> = VecDeque::new();
            queue.push_back((symbol.clone(), 0));

            while let Some((current, depth)) = queue.pop_front() {
                if depth >= max_depth || visited.contains(&current) {
                    continue;
                }
                visited.insert(current.clone());

                // Find where this symbol is defined
                let defs = queries::find_definitions(conn, &current, None)?;

                for def in &defs {
                    // Find what this symbol's body references
                    let file_symbols = queries::get_file_symbols(conn, &def.file_path)?;

                    // Get refs in the same file that fall within this symbol's line range
                    let refs = queries::find_references(conn, &current, Some(&def.file_path), 100)?;
                    let _ = refs; // We use the inverse: find what this definition calls

                    // Get all references FROM the file, within the definition's line range
                    let all_refs_in_file =
                        queries::find_references(conn, "%", Some(&def.file_path), 500)
                            .unwrap_or_default();

                    for r in &all_refs_in_file {
                        if r.start_line >= def.start_line
                            && r.start_line <= (def.start_line + (def.end_line - def.start_line))
                            && r.symbol_name != current
                        {
                            edges.push((
                                def.file_path.clone(),
                                current.clone(),
                                r.symbol_name.clone(),
                            ));
                            if !visited.contains(&r.symbol_name) {
                                queue.push_back((r.symbol_name.clone(), depth + 1));
                            }
                        }
                    }

                    // Also check: who calls this symbol?
                    let callers = queries::find_references(conn, &current, None, 50)?;
                    for caller in &callers {
                        // Find which symbol contains this reference
                        for fs in &file_symbols {
                            if caller.file_path == def.file_path
                                && caller.start_line >= fs.start_line
                                && caller.start_line <= fs.end_line
                                && fs.name != current
                            {
                                edges.push((
                                    caller.file_path.clone(),
                                    fs.name.clone(),
                                    current.clone(),
                                ));
                            }
                        }
                    }
                }
            }

            Ok::<_, anyhow::Error>(edges)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("call graph error: {e}"), None))?;

    if graph.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No call graph data found for '{}'", args.symbol),
        )]));
    }

    // Deduplicate edges
    let unique_edges: BTreeSet<_> = graph.into_iter().collect();

    let mut output = format!("# Call Graph for `{}`\n\n", args.symbol);
    output.push_str("```\n");
    for (file, caller, callee) in &unique_edges {
        output.push_str(&format!("{} -> {} (in {})\n", caller, callee, file));
    }
    output.push_str("```\n");

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

// ── Dependency tree tool ────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetDependencyTreeArgs {
    #[schemars(description = "Relative file path to build the dependency tree for")]
    pub file_path: String,
    #[schemars(description = "Maximum depth to traverse (default: 3, max: 10)")]
    pub depth: Option<u32>,
    #[schemars(
        description = "Direction: 'imports' (what this file depends on) or 'importers' (what depends on this file). Default: 'imports'"
    )]
    pub direction: Option<String>,
}

pub async fn get_dependency_tree(
    state: &AppState,
    args: GetDependencyTreeArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let db = Arc::clone(&state.db);
    let file_path = args.file_path.clone();
    let max_depth = args.depth.unwrap_or(3).min(10);
    let direction = args
        .direction
        .clone()
        .unwrap_or_else(|| "imports".to_string());
    let direction_clone = direction.clone();

    let tree = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let mut result: BTreeMap<String, Vec<String>> = BTreeMap::new();
            let mut visited: BTreeSet<String> = BTreeSet::new();
            let mut queue: VecDeque<(String, u32)> = VecDeque::new();
            queue.push_back((file_path.clone(), 0));

            while let Some((current, depth)) = queue.pop_front() {
                if depth >= max_depth || visited.contains(&current) {
                    continue;
                }
                visited.insert(current.clone());

                if direction_clone == "imports" {
                    // What does this file import?
                    let imports = queries::get_file_imports(conn, &current)?;
                    let deps: Vec<String> = imports.iter().map(|i| i.source_path.clone()).collect();
                    for dep in &deps {
                        if !visited.contains(dep) {
                            queue.push_back((dep.clone(), depth + 1));
                        }
                    }
                    result.insert(current, deps);
                } else {
                    // What imports this file? (reverse lookup)
                    let importers = importers_for_path(conn, &current)?;
                    for imp in &importers {
                        if !visited.contains(imp) {
                            queue.push_back((imp.clone(), depth + 1));
                        }
                    }
                    result.insert(current, importers);
                }
            }

            Ok::<_, anyhow::Error>(result)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("dependency tree error: {e}"), None))?;

    if tree.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No dependency data found for '{}'", args.file_path),
        )]));
    }

    let dir_label = if direction == "imports" {
        "depends on"
    } else {
        "imported by"
    };
    let mut output = format!("# Dependency Tree for `{}`\n\n", args.file_path);
    output.push_str(&format!("Direction: {}\n\n", dir_label));

    for (file, deps) in &tree {
        if deps.is_empty() {
            output.push_str(&format!("- `{file}` (leaf)\n"));
        } else {
            output.push_str(&format!("- `{file}` {} {}\n", dir_label, deps.len()));
            for dep in deps {
                output.push_str(&format!("  - `{dep}`\n"));
            }
        }
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

// ── Type hierarchy tool ─────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetTypeHierarchyArgs {
    #[schemars(
        description = "Type name (class, struct, interface, trait) to get the hierarchy for"
    )]
    pub type_name: String,
}

pub async fn get_type_hierarchy(
    state: &AppState,
    args: GetTypeHierarchyArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let db = Arc::clone(&state.db);
    let type_name = args.type_name.clone();

    let hierarchy = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            // Find the type definition
            let defs = queries::find_definitions(conn, &type_name, None)?;
            let type_kinds: BTreeSet<&str> = [
                "class",
                "struct",
                "interface",
                "trait",
                "enum",
                "type_alias",
            ]
            .iter()
            .copied()
            .collect();

            let mut output_parts: Vec<String> = Vec::new();

            for def in &defs {
                if !type_kinds.contains(def.kind.as_str()) {
                    continue;
                }

                output_parts.push(format!(
                    "**{}** ({}) in `{}` at line {}",
                    def.name, def.kind, def.file_path, def.start_line
                ));

                // Find symbols that reference this type (potential implementations/extensions)
                let refs = queries::find_references(conn, &type_name, None, 100)?;
                let mut implementors: BTreeSet<String> = BTreeSet::new();

                for r in &refs {
                    if r.kind == "implements" || r.kind == "extends" || r.kind == "impl" {
                        // Find the enclosing type for this reference
                        let file_syms = queries::get_file_symbols(conn, &r.file_path)?;
                        for fs in &file_syms {
                            if type_kinds.contains(fs.kind.as_str())
                                && r.start_line >= fs.start_line
                                && r.start_line <= fs.end_line
                            {
                                implementors.insert(format!(
                                    "{} ({}) in `{}`",
                                    fs.name, fs.kind, r.file_path
                                ));
                            }
                        }
                    }
                }

                if !implementors.is_empty() {
                    output_parts.push("Implementations/Subtypes:".to_string());
                    for imp in &implementors {
                        output_parts.push(format!("  - {}", imp));
                    }
                }

                // Find child symbols (methods, fields) within this type
                let file_syms = queries::get_file_symbols(conn, &def.file_path)?;
                let mut children: Vec<String> = Vec::new();
                for fs in &file_syms {
                    if fs.start_line > def.start_line
                        && fs.end_line <= def.end_line
                        && fs.name != def.name
                    {
                        children.push(format!("{} ({}) line {}", fs.name, fs.kind, fs.start_line));
                    }
                }
                if !children.is_empty() {
                    output_parts.push("Members:".to_string());
                    for child in &children {
                        output_parts.push(format!("  - {}", child));
                    }
                }
            }

            Ok::<_, anyhow::Error>(output_parts)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("type hierarchy error: {e}"), None))?;

    if hierarchy.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No type hierarchy found for '{}'", args.type_name),
        )]));
    }

    let mut output = format!("# Type Hierarchy for `{}`\n\n", args.type_name);
    for part in &hierarchy {
        output.push_str(part);
        output.push('\n');
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}
