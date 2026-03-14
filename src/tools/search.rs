use std::time::Duration;

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::db::queries;
use crate::state::AppState;

/// Maximum time for a blocking query before we return a timeout error.
const QUERY_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchCodeArgs {
    #[schemars(
        description = "Full-text search query to find in code content (FTS5 syntax supported)"
    )]
    pub query: String,
    #[schemars(description = "Optional: filter by language (e.g. 'rust', 'python')")]
    pub language: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 20, max: 100)")]
    pub limit: Option<u32>,
    #[schemars(description = "Number of results to skip for pagination (default: 0)")]
    pub offset: Option<u32>,
}

pub async fn search_code(
    state: &AppState,
    args: SearchCodeArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.query, "query")?;

    let db = state.db.clone();
    let query = args.query.clone();
    let language = args.language.clone();
    let limit = args.limit.unwrap_or(20).min(100);
    let offset = args.offset.unwrap_or(0);

    let results = tokio::time::timeout(
        QUERY_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| {
                Ok(queries::search_fts(
                    conn,
                    &query,
                    language.as_deref(),
                    limit,
                    offset,
                )?)
            })
        }),
    )
    .await
    .map_err(|_| rmcp::ErrorData::internal_error("search query timed out (30s limit)", None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("search error: {e}"), None))?;

    if results.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No results found for query: '{}'", args.query),
        )]));
    }

    let mut output = format!("Found {} results for '{}':\n\n", results.len(), args.query);
    for r in &results {
        // Count line number based on content position (approximate from snippet)
        output.push_str(&format!(
            "## {} ({})\n**Score:** {:.2}\n```\n{}\n```\n\n",
            r.file_path, r.language, r.rank, r.snippet,
        ));
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchSymbolsArgs {
    #[schemars(description = "Symbol name or pattern to search for")]
    pub name: String,
    #[schemars(
        description = "Optional: filter by symbol kind (e.g. 'function', 'class', 'struct')"
    )]
    pub kind: Option<String>,
    #[schemars(description = "Optional: filter by language")]
    pub language: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 20, max: 100)")]
    pub limit: Option<usize>,
}

pub async fn search_symbols(
    state: &AppState,
    args: SearchSymbolsArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.name, "name")?;

    let db = state.db.clone();
    let name = args.name.clone();
    let kind = args.kind.clone();
    let language = args.language.clone();
    let limit = args.limit.unwrap_or(20).min(100);

    let results = tokio::time::timeout(
        QUERY_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| {
                Ok(queries::search_symbols(
                    conn,
                    &name,
                    kind.as_deref(),
                    language.as_deref(),
                    limit,
                )?)
            })
        }),
    )
    .await
    .map_err(|_| rmcp::ErrorData::internal_error("symbol search timed out (30s limit)", None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("symbol search error: {e}"), None))?;

    if results.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No symbols found matching: '{}'", args.name),
        )]));
    }

    let mut output = format!(
        "Found {} symbols matching '{}':\n\n",
        results.len(),
        args.name
    );
    for s in &results {
        let doc = s
            .doc_comment
            .as_deref()
            .map(|d| format!("\n  Doc: {}", d))
            .unwrap_or_default();
        output.push_str(&format!(
            "- **{}** ({}) in `{}` at line {}{}\n  Scope: {}\n\n",
            s.name,
            s.kind,
            s.file_path,
            s.start_line,
            doc,
            s.scope_path.as_deref().unwrap_or("-"),
        ));
    }

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        output,
    )]))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
pub struct SemanticSearchArgs {
    #[schemars(description = "Natural language query for semantic search")]
    pub query: String,
    #[schemars(description = "Maximum number of results (default: 10)")]
    pub limit: Option<usize>,
}

pub async fn semantic_search(
    state: &AppState,
    args: SemanticSearchArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    #[cfg(not(feature = "semantic"))]
    {
        let _ = (state, args);
        Err(rmcp::ErrorData::internal_error(
            "Semantic search is not available. Rebuild with --features semantic to enable it.",
            None,
        ))
    }

    #[cfg(feature = "semantic")]
    {
        let semantic_mutex = state.semantic.as_ref().as_ref().ok_or_else(|| {
            rmcp::ErrorData::internal_error("Semantic engine not initialized", None)
        })?;
        let mut semantic = semantic_mutex.lock().await;

        let limit = args.limit.unwrap_or(10);
        let results = semantic
            .search(&args.query, limit, &state.db)
            .await
            .map_err(|e| {
                rmcp::ErrorData::internal_error(format!("semantic search error: {e}"), None)
            })?;

        if results.is_empty() {
            return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
                format!("No semantic results found for: '{}'", args.query),
            )]));
        }

        let mut output = format!(
            "Found {} semantic matches for '{}':\n\n",
            results.len(),
            args.query
        );
        for r in &results {
            output.push_str(&format!(
                "## {} (similarity: {:.3})\n```\n{}\n```\n\n",
                r.file_path, r.score, r.snippet,
            ));
        }

        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            output,
        )]))
    }
}

// ── Regex search tool ───────────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct SearchByRegexArgs {
    #[schemars(description = "Regex pattern to search for in source code (Rust regex syntax)")]
    pub pattern: String,
    #[schemars(description = "Optional: filter by language")]
    pub language: Option<String>,
    #[schemars(description = "Maximum number of results to return (default: 20, max: 100)")]
    pub limit: Option<u32>,
}

pub async fn search_by_regex(
    state: &AppState,
    args: SearchByRegexArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    super::require_non_empty(&args.pattern, "pattern")?;

    // Validate the regex pattern upfront so the user gets a clear error
    // instead of a generic database/internal error.
    if let Err(e) = regex::Regex::new(&args.pattern) {
        return Err(rmcp::ErrorData::invalid_params(
            format!("invalid regex pattern: {e}"),
            None,
        ));
    }

    let db = state.db.clone();
    let pattern = args.pattern.clone();
    let language = args.language.clone();
    let limit = args.limit.unwrap_or(20).min(100);

    let results = tokio::time::timeout(
        QUERY_TIMEOUT,
        tokio::task::spawn_blocking(move || {
            db.with_conn(|conn| {
                Ok(queries::search_by_regex(
                    conn,
                    &pattern,
                    language.as_deref(),
                    limit,
                )?)
            })
        }),
    )
    .await
    .map_err(|_| rmcp::ErrorData::internal_error("regex search timed out (30s limit)", None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
    .map_err(|e| rmcp::ErrorData::internal_error(format!("regex search error: {e}"), None))?;

    if results.is_empty() {
        return Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("No regex matches found for pattern: '{}'", args.pattern),
        )]));
    }

    let mut output = format!(
        "Found {} regex matches for '{}':\n\n",
        results.len(),
        args.pattern
    );
    for r in &results {
        output.push_str(&format!(
            "- `{}` ({}):\n  ```\n  {}\n  ```\n\n",
            r.file_path, r.language, r.snippet,
        ));
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
                crate::db::queries::upsert_fts(
                    conn,
                    file_id,
                    "src/main.rs",
                    "test",
                    "fn test() {}\n// Some comment",
                    "rust",
                )
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
                Ok::<_, anyhow::Error>(())
            })
            .unwrap();
        state
    }

    #[tokio::test]
    async fn test_search_code_empty() {
        let state = setup_state().await;
        let args = SearchCodeArgs {
            query: "test".to_string(),
            language: None,
            limit: None,
            offset: None,
        };

        let result = search_code(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("No results found"));
    }

    #[tokio::test]
    async fn test_search_symbols_empty() {
        let state = setup_state().await;
        let args = SearchSymbolsArgs {
            name: "MyClass".to_string(),
            kind: None,
            language: None,
            limit: None,
        };

        let result = search_symbols(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("No symbols found"));
    }

    #[tokio::test]
    async fn test_search_by_regex_empty() {
        let state = setup_state().await;
        let args = SearchByRegexArgs {
            pattern: "^fn test.*".to_string(),
            language: None,
            limit: None,
        };

        let result = search_by_regex(&state, args).await.unwrap();
        assert_eq!(result.content.len(), 1);
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("No regex matches found"));
    }

    #[tokio::test]
    async fn test_search_code_populated() {
        let state = setup_populated_state().await;
        let args = SearchCodeArgs {
            query: "test".to_string(),
            language: None,
            limit: None,
            offset: None,
        };

        let result = search_code(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Found 1 results for 'test'"));
        assert!(text.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_search_symbols_populated() {
        let state = setup_populated_state().await;
        let args = SearchSymbolsArgs {
            name: "test".to_string(),
            kind: None,
            language: None,
            limit: None,
        };

        let result = search_symbols(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Found 1 symbols matching 'test'"));
        assert!(text.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_search_by_regex_populated() {
        let state = setup_populated_state().await;
        let args = SearchByRegexArgs {
            pattern: "^fn test.*".to_string(),
            language: None,
            limit: None,
        };

        let result = search_by_regex(&state, args).await.unwrap();
        let text = format!("{:?}", result.content[0]);
        assert!(text.contains("Found 1 regex matches for '^fn test.*'"));
        assert!(text.contains("src/main.rs"));
    }

    #[tokio::test]
    async fn test_search_code_empty_query_rejected() {
        let state = setup_state().await;
        let args = SearchCodeArgs {
            query: "  ".to_string(),
            language: None,
            limit: None,
            offset: None,
        };
        let result = search_code(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_search_symbols_empty_name_rejected() {
        let state = setup_state().await;
        let args = SearchSymbolsArgs {
            name: "".to_string(),
            kind: None,
            language: None,
            limit: None,
        };
        let result = search_symbols(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }

    #[tokio::test]
    async fn test_search_by_regex_invalid_pattern() {
        let state = setup_state().await;
        let args = SearchByRegexArgs {
            pattern: "[invalid".to_string(),
            language: None,
            limit: None,
        };
        let result = search_by_regex(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("invalid regex pattern"));
    }

    #[tokio::test]
    async fn test_search_by_regex_empty_pattern_rejected() {
        let state = setup_state().await;
        let args = SearchByRegexArgs {
            pattern: "".to_string(),
            language: None,
            limit: None,
        };
        let result = search_by_regex(&state, args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("must not be empty"));
    }
}
