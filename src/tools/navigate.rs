use std::sync::Arc;

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::db::queries;
use crate::state::AppState;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindDefinitionArgs {
    #[schemars(description = "The symbol name to find the definition of")]
    pub symbol: String,
    #[schemars(description = "Optional: hint file path to narrow the search scope")]
    pub file_hint: Option<String>,
}

pub async fn find_definition(
    state: &AppState,
    args: FindDefinitionArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.symbol, "symbol")?;

    let db = Arc::clone(&state.db);
    let symbol = args.symbol.clone();
    let file_hint = args.file_hint.clone();

    let definitions = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            Ok(queries::find_definitions(
                conn,
                &symbol,
                file_hint.as_deref(),
            )?)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("find_definition error: {e}"), None))?;

    if definitions.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No definition found for symbol: '{}'", args.symbol),
        )]));
    }

    let mut output = format!(
        "Found {} definition(s) for '{}':\n\n",
        definitions.len(),
        args.symbol,
    );

    let db = Arc::clone(&state.db);
    let defs = definitions.clone();
    let enriched = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            let mut results = Vec::new();
            for d in &defs {
                let content = queries::get_file_content(conn, &d.file_path).unwrap_or(None);
                results.push((d.clone(), content));
            }
            Ok::<_, anyhow::Error>(results)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("content fetch error: {e}"), None))?;

    for (def, content) in &enriched {
        output.push_str(&format!(
            "## {} ({}) in `{}`\n",
            def.name, def.kind, def.file_path,
        ));
        output.push_str(&format!("**Line:** {}\n", def.start_line));
        if let Some(doc) = &def.doc_comment {
            output.push_str(&format!("**Doc:** {}\n", doc));
        }

        // Show surrounding source context (±10 lines)
        if let Some(src) = content {
            let lines: Vec<&str> = src.lines().collect();
            let line = def.start_line as usize;
            let start = line.saturating_sub(10).max(1);
            let end = (line + 10).min(lines.len());
            if start <= end && start >= 1 {
                let lang = def.file_path.rsplit('.').next().unwrap_or("");
                output.push_str(&format!("\n```{}\n", lang));
                for (i, l) in lines[start - 1..end].iter().enumerate() {
                    let line_num = start + i;
                    let marker = if line_num == line { "→" } else { " " };
                    output.push_str(&format!("{} {:>4} | {}\n", marker, line_num, l));
                }
                output.push_str("```\n\n");
            }
        }
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindReferencesArgs {
    #[schemars(description = "The symbol name to find references for")]
    pub symbol: String,
    #[schemars(description = "Optional: hint file path to narrow the search scope")]
    pub file_hint: Option<String>,
    #[schemars(description = "Maximum number of references (default: 30)")]
    pub limit: Option<u32>,
}

pub async fn find_references(
    state: &AppState,
    args: FindReferencesArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.symbol, "symbol")?;

    let db = Arc::clone(&state.db);
    let symbol = args.symbol.clone();
    let file_hint = args.file_hint.clone();
    // Cap at 100 for consistency with other search/list tools
    let limit = args.limit.unwrap_or(30).min(100);

    let references = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| {
            Ok(queries::find_references(
                conn,
                &symbol,
                file_hint.as_deref(),
                limit,
            )?)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("find_references error: {e}"), None))?;

    if references.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No references found for symbol: '{}'", args.symbol),
        )]));
    }

    let mut output = format!(
        "Found {} reference(s) for '{}':\n\n",
        references.len(),
        args.symbol,
    );

    // Enrich references with ±3 lines of context
    let db_ctx = Arc::clone(&state.db);
    let refs_for_ctx = references.clone();
    let enriched = tokio::task::spawn_blocking(move || {
        db_ctx.with_conn(|conn| {
            let mut ctx_map: std::collections::HashMap<String, Option<String>> =
                std::collections::HashMap::new();
            for r in &refs_for_ctx {
                if !ctx_map.contains_key(&r.file_path) {
                    ctx_map.insert(
                        r.file_path.clone(),
                        queries::get_file_content(conn, &r.file_path)?,
                    );
                }
            }
            Ok::<_, anyhow::Error>(ctx_map)
        })
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("context fetch error: {e}"), None))?;

    for r in &references {
        output.push_str(&format!(
            "- `{}` line {} ({})\n",
            r.file_path, r.start_line, r.kind,
        ));
        // Add ±3 lines context if available
        if let Some(Some(content)) = enriched.get(&r.file_path) {
            let lines: Vec<&str> = content.lines().collect();
            let line = r.start_line as usize;
            let start = line.saturating_sub(3).max(1);
            let end = (line + 3).min(lines.len());
            if start >= 1 && start <= end {
                output.push_str("  ```\n");
                for (i, l) in lines[start - 1..end].iter().enumerate() {
                    let ln = start + i;
                    let marker = if ln == line { ">" } else { " " };
                    output.push_str(&format!("  {} {:>4} | {}\n", marker, ln, l));
                }
                output.push_str("  ```\n");
            }
        }
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct GetImportsArgs {
    #[schemars(description = "Relative file path to get import/dependency information for")]
    pub file_path: String,
}

pub async fn get_imports(
    state: &AppState,
    args: GetImportsArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.file_path, "file_path")?;

    let db = Arc::clone(&state.db);
    let file_path = args.file_path.clone();

    let imports = tokio::task::spawn_blocking(move || {
        db.with_conn(|conn| Ok(queries::get_file_imports(conn, &file_path)?))
    })
    .await
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("get_imports error: {e}"), None))?;

    if imports.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No imports found in: '{}'", args.file_path),
        )]));
    }

    let mut output = format!("Imports in '{}':\n\n", args.file_path);
    for imp in &imports {
        let names = imp
            .imported_names
            .as_deref()
            .map(|n| format!(" ({})", n))
            .unwrap_or_default();
        output.push_str(&format!("- `{}`{}\n", imp.source_path, names));
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
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
    async fn test_find_definition_empty() {
        let state = setup_state().await;
        let args = FindDefinitionArgs {
            symbol: "NonExistent".to_string(),
            file_hint: None,
        };

        let result = find_definition(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("No definition found"));
    }

    #[tokio::test]
    async fn test_find_references_empty() {
        let state = setup_state().await;
        let args = FindReferencesArgs {
            symbol: "NonExistent".to_string(),
            file_hint: None,
            limit: None,
        };

        let result = find_references(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("No references found"));
    }

    #[tokio::test]
    async fn test_get_imports_empty() {
        let state = setup_state().await;
        let args = GetImportsArgs {
            file_path: "missing.rs".to_string(),
        };

        let result = get_imports(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("No imports found"));
    }

    #[tokio::test]
    async fn test_find_definition_populated() {
        let state = setup_populated_state().await;
        let args = FindDefinitionArgs {
            symbol: "test".to_string(),
            file_hint: None,
        };
        let result = find_definition(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Found 1 definition(s) for 'test'"));
        assert!(text.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_find_references_populated() {
        let state = setup_populated_state().await;
        let args = FindReferencesArgs {
            symbol: "test".to_string(),
            file_hint: None,
            limit: None,
        };
        let result = find_references(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Found 1 reference(s) for 'test'"));
        assert!(text.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_get_imports_populated() {
        let state = setup_populated_state().await;
        let args = GetImportsArgs {
            file_path: "src/main.rs".to_string(),
        };
        let result = get_imports(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Imports in 'src/main.rs'"));
        assert!(text.contains("std::collections::HashMap"));
    }

    #[tokio::test]
    async fn test_find_definition_empty_symbol_rejected() {
        let state = setup_state().await;
        let args = FindDefinitionArgs {
            symbol: "".to_string(),
            file_hint: None,
        };
        let result = find_definition(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_find_references_empty_symbol_rejected() {
        let state = setup_state().await;
        let args = FindReferencesArgs {
            symbol: " ".to_string(),
            file_hint: None,
            limit: None,
        };
        let result = find_references(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_get_imports_empty_path_rejected() {
        let state = setup_state().await;
        let args = GetImportsArgs {
            file_path: "".to_string(),
        };
        let result = get_imports(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }
}
