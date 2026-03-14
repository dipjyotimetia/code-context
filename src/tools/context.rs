use std::sync::Arc;

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::db::queries;
use crate::state::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileSummaryArgs {
    #[schemars(description = "Relative path of the file to summarize")]
    pub path: String,
}

pub async fn get_file_summary(
    state: &AppState,
    args: GetFileSummaryArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.path, "path")?;

    let db = Arc::clone(&state.db);
    let path = args.path.clone();

    let (file_record, symbols, imports) = {
        let db2 = Arc::clone(&db);
        let db3 = Arc::clone(&db);
        let p1 = path.clone();
        let p2 = path.clone();
        let p3 = path.clone();

        let (fr, syms, imps) = tokio::try_join!(
            tokio::task::spawn_blocking(move || {
                db.with_conn(|conn| Ok(queries::get_file_record(conn, &p1)?))
            }),
            tokio::task::spawn_blocking(move || {
                db2.with_conn(|conn| Ok(queries::get_file_symbols(conn, &p2)?))
            }),
            tokio::task::spawn_blocking(move || {
                db3.with_conn(|conn| Ok(queries::get_file_imports(conn, &p3)?))
            }),
        )
        .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?;

        (
            fr.map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?,
            syms.map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?,
            imps.map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?,
        )
    };

    let file = file_record.ok_or_else(|| {
        rmcp::ErrorData::invalid_params(format!("File not indexed: '{}'", args.path), None)
    })?;

    let mut output = format!("# File: {}\n\n", file.path);
    output.push_str(&format!("- **Language:** {}\n", file.language));
    output.push_str(&format!("- **Size:** {} bytes\n", file.size_bytes));
    output.push_str(&format!("- **Last indexed:** {}\n\n", file.indexed_at));

    // Imports
    if !imports.is_empty() {
        output.push_str("## Imports\n\n");
        for imp in &imports {
            output.push_str(&format!("- `{}`\n", imp.source_path));
        }
        output.push('\n');
    }

    // Symbols grouped by kind
    if !symbols.is_empty() {
        output.push_str("## Symbols\n\n");

        let mut by_kind: std::collections::BTreeMap<String, Vec<&queries::SymbolInfo>> =
            std::collections::BTreeMap::new();
        for s in &symbols {
            by_kind.entry(s.kind.clone()).or_default().push(s);
        }

        for (kind, syms) in &by_kind {
            output.push_str(&format!("### {}\n\n", capitalize(kind)));
            for s in syms {
                let doc = s
                    .doc_comment
                    .as_deref()
                    .map(|d| format!(" — {}", d.lines().next().unwrap_or("")))
                    .unwrap_or_default();
                output.push_str(&format!(
                    "- **{}** (line {}){}\n",
                    s.name, s.start_line, doc,
                ));
            }
            output.push('\n');
        }
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetSymbolContextArgs {
    #[schemars(description = "Symbol name to get full context for")]
    pub symbol: String,
    #[schemars(
        description = "Number of lines of context around each definition/reference (default: 15)"
    )]
    pub context_lines: Option<usize>,
}

pub async fn get_symbol_context(
    state: &AppState,
    args: GetSymbolContextArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.symbol, "symbol")?;

    // Cap at 50 to prevent excessively large responses
    let ctx_lines = args.context_lines.unwrap_or(15).min(50);
    let db = Arc::clone(&state.db);
    let symbol = args.symbol.clone();

    let db2 = Arc::clone(&db);
    let sym2 = symbol.clone();

    let (definitions, references) = tokio::try_join!(
        tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| Ok(queries::find_definitions(conn, &symbol, None)?))
        }),
        tokio::task::spawn_blocking(move || {
            db2.with_conn(|conn| Ok(queries::find_references(conn, &sym2, None, 50)?))
        }),
    )
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?;

    let definitions =
        definitions.map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?;
    let references =
        references.map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?;

    if definitions.is_empty() && references.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("Symbol '{}' not found in the index", args.symbol),
        )]));
    }

    let mut output = format!("# Symbol Context: `{}`\n\n", args.symbol);

    // Definitions with source context
    if !definitions.is_empty() {
        output.push_str("## Definitions\n\n");
        for def in &definitions {
            output.push_str(&format!(
                "### {} ({}) — `{}`\n",
                def.name, def.kind, def.file_path,
            ));
            if let Some(doc) = &def.doc_comment {
                output.push_str(&format!("**Doc:** {}\n", doc));
            }

            // Fetch source context
            let db = Arc::clone(&state.db);
            let fp = def.file_path.clone();
            let line = def.start_line as usize;
            let cl = ctx_lines;

            let content = tokio::task::spawn_blocking(move || {
                db.with_conn(|conn| Ok(queries::get_file_content(conn, &fp)?))
            })
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?;

            if let Some(src) = content {
                let lines: Vec<&str> = src.lines().collect();
                let start = line.saturating_sub(cl).max(1);
                let end = (line + cl).min(lines.len());
                if start >= 1 && start <= end {
                    let ext = def.file_path.rsplit('.').next().unwrap_or("");
                    output.push_str(&format!("\n```{}\n", ext));
                    for (i, l) in lines[start - 1..end].iter().enumerate() {
                        let ln = start + i;
                        let marker = if ln == line { "→" } else { " " };
                        output.push_str(&format!("{} {:>4} | {}\n", marker, ln, l));
                    }
                    output.push_str("```\n\n");
                }
            }
        }
    }

    // References summary
    if !references.is_empty() {
        output.push_str("## References\n\n");
        // Group by file
        let mut by_file: std::collections::BTreeMap<&str, Vec<&queries::ReferenceLocation>> =
            std::collections::BTreeMap::new();
        for r in &references {
            by_file.entry(&r.file_path).or_default().push(r);
        }
        for (file, refs) in &by_file {
            let lines: Vec<String> = refs.iter().map(|r| r.start_line.to_string()).collect();
            output.push_str(&format!("- `{}` — lines: {}\n", file, lines.join(", "),));
        }
        output.push('\n');
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

pub async fn get_project_overview(state: &AppState) -> Result<CallToolResult, rmcp::ErrorData> {
    let db = Arc::clone(&state.db);

    let stats = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| Ok(queries::get_project_stats(conn)?))
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("stats error: {e}"), None))?;

    // Report watcher status without holding the lock for longer than needed.
    let watcher_status = {
        let guard = state.watcher.lock().await;
        match &*guard {
            Some(w) => format!("active (watching {})", w.root().display()),
            None => "inactive — call watch_repository to keep index current".to_string(),
        }
    };

    let mut output = String::from("# Project Overview\n\n");
    output.push_str(&format!("- **Total files:** {}\n", stats.total_files));
    output.push_str(&format!("- **Total symbols:** {}\n", stats.total_symbols));
    output.push_str(&format!("- **Total references:** {}\n", stats.total_refs));
    output.push_str(&format!(
        "- **Total size:** {:.1} KB\n",
        stats.total_size_bytes as f64 / 1024.0,
    ));
    output.push_str(&format!("- **File watcher:** {}\n\n", watcher_status));

    if !stats.languages.is_empty() {
        output.push_str("## Languages\n\n");
        output.push_str("| Language | Files |\n|---|---|\n");
        for (lang, count) in &stats.languages {
            output.push_str(&format!("| {} | {} |\n", lang, count));
        }
        output.push('\n');
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().to_string() + c.as_str(),
    }
}

// ── File changes tool ───────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetFileChangesArgs {
    #[schemars(description = "Relative file path to check for changes since last index")]
    pub path: String,
}

pub async fn get_file_changes(
    state: &AppState,
    args: GetFileChangesArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.path, "path")?;

    let db = Arc::clone(&state.db);
    let path = args.path.clone();

    let result = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let record = queries::get_file_record(conn, &path)?;

            match record {
                None => Ok(format!("File '{}' is not yet indexed.", path)),
                Some(rec) => {
                    // Check if file exists on disk
                    let content = queries::get_file_content(conn, &path)?;
                    let info = format!(
                        "# File Status: {}\n\n\
                         - **Language:** {}\n\
                         - **Size:** {} bytes\n\
                         - **Last indexed:** {}\n\
                         - **Content stored:** {}\n",
                        rec.path,
                        rec.language,
                        rec.size_bytes,
                        rec.indexed_at,
                        if content.is_some() { "yes" } else { "no" },
                    );
                    Ok(info)
                }
            }
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("{e}"), None))?;

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        result,
    )]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::indexer::languages::LanguageRegistry;
    use std::path::PathBuf;

    async fn setup_state() -> AppState {
        let db = Database::init(&PathBuf::from(":memory:")).unwrap();
        let registry = LanguageRegistry::new();
        AppState::new(db, registry)
    }

    async fn setup_populated_state() -> AppState {
        let state = setup_state().await;
        state
            .db
            .with_conn(|conn| {
                let file_id =
                    crate::db::queries::upsert_file(conn, "src/main.rs", "hash", "rust", 100)
                        .unwrap();
                crate::db::queries::upsert_content(conn, file_id, "fn test() {}\n// Some comment")
                    .unwrap();
                crate::db::queries::insert_symbols_batch(
                    conn,
                    file_id,
                    &[crate::db::queries::SymbolDef {
                        name: "test".to_string(),
                        kind: "function".to_string(),
                        start_line: 1,
                        start_col: 0,
                        end_line: 1,
                        end_col: 12,
                        parent_id: None,
                        scope_path: None,
                        doc_comment: Some("Test function".to_string()),
                    }],
                )
                .unwrap();
                crate::db::queries::insert_refs_batch(
                    conn,
                    file_id,
                    &[crate::db::queries::SymbolRef {
                        symbol_name: "test".to_string(),
                        kind: "function_call".to_string(),
                        start_line: 2,
                        start_col: 0,
                    }],
                )
                .unwrap();
                crate::db::queries::insert_imports_batch(
                    conn,
                    file_id,
                    &[crate::db::queries::ImportRecord {
                        source_path: "std::collections::HashMap".to_string(),
                        imported_names: Some("HashMap".to_string()),
                    }],
                )
                .unwrap();
                Ok::<_, anyhow::Error>(())
            })
            .unwrap();
        state
    }

    #[tokio::test]
    async fn test_get_file_summary_missing() {
        let state = setup_state().await;
        let args = GetFileSummaryArgs {
            path: "nonexistent.rs".to_string(),
        };

        let result = get_file_summary(&state, args).await;
        assert!(result.is_err());
        if let Err(e) = result {
            assert!(e.message.contains("File not indexed"));
        }
    }

    #[tokio::test]
    async fn test_get_symbol_context_missing() {
        let state = setup_state().await;
        let args = GetSymbolContextArgs {
            symbol: "MissingSymbol".to_string(),
            context_lines: None,
        };

        let result = get_symbol_context(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("not found in the index"));
    }

    #[tokio::test]
    async fn test_get_project_overview_empty() {
        let state = setup_state().await;
        let result = get_project_overview(&state).await.unwrap();

        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Total files:** 0"));
        assert!(text.contains("Total symbols:** 0"));
    }

    #[tokio::test]
    async fn test_get_file_changes_missing() {
        let state = setup_state().await;
        let args = GetFileChangesArgs {
            path: "unknown.rs".to_string(),
        };

        let result = get_file_changes(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("is not yet indexed"));
    }

    #[tokio::test]
    async fn test_get_file_summary_populated() {
        let state = setup_populated_state().await;
        let args = GetFileSummaryArgs {
            path: "src/main.rs".to_string(),
        };
        let result = get_file_summary(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Language:** rust"));
        assert!(text.contains("std::collections::HashMap"));
        assert!(text.contains("test"));
    }

    #[tokio::test]
    async fn test_get_symbol_context_populated() {
        let state = setup_populated_state().await;
        let args = GetSymbolContextArgs {
            symbol: "test".to_string(),
            context_lines: None,
        };
        let result = get_symbol_context(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Symbol Context: `test`"));
        assert!(text.contains("Definitions"));
        assert!(text.contains("References"));
    }

    #[tokio::test]
    async fn test_get_project_overview_populated() {
        let state = setup_populated_state().await;
        let result = get_project_overview(&state).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Total files:** 1"));
        assert!(text.contains("Total symbols:** 1"));
    }

    #[tokio::test]
    async fn test_get_file_changes_populated() {
        let state = setup_populated_state().await;
        let args = GetFileChangesArgs {
            path: "src/main.rs".to_string(),
        };
        let result = get_file_changes(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("File Status: src/main.rs"));
    }

    #[tokio::test]
    async fn test_get_file_summary_empty_path_rejected() {
        let state = setup_state().await;
        let args = GetFileSummaryArgs {
            path: "".to_string(),
        };
        let result = get_file_summary(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_get_symbol_context_empty_symbol_rejected() {
        let state = setup_state().await;
        let args = GetSymbolContextArgs {
            symbol: " ".to_string(),
            context_lines: None,
        };
        let result = get_symbol_context(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_get_file_changes_empty_path_rejected() {
        let state = setup_state().await;
        let args = GetFileChangesArgs {
            path: "".to_string(),
        };
        let result = get_file_changes(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }
}
