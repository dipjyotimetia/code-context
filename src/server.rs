use rmcp::{
    Peer, RoleServer, ServerHandler,
    handler::server::{
        router::{prompt::PromptRouter, tool::ToolRouter},
        wrapper::Parameters,
    },
    model::{
        CallToolResult, CompleteRequestParams, CompleteResult, CompletionInfo, GetPromptRequestParams,
        GetPromptResult, ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, Meta,
        PaginatedRequestParams, ReadResourceRequestParams, ReadResourceResult, ServerCapabilities,
        ServerInfo,
    },
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};

use crate::prompts;
use crate::resources;
use crate::state::AppState;
use crate::tools::{context, graph, indexing, navigate, search};

#[derive(Clone)]
pub struct CodeContextServer {
    state: AppState,
    #[allow(dead_code)]
    tool_router: ToolRouter<Self>,
    #[allow(dead_code)]
    prompt_router: PromptRouter<Self>,
}

impl CodeContextServer {
    pub fn new(state: AppState) -> Self {
        let tool_router = Self::tool_router();
        let prompt_router = Self::prompt_router();
        Self {
            state,
            tool_router,
            prompt_router,
        }
    }
}

#[tool_router]
impl CodeContextServer {
    // ── Indexing tools ──────────────────────────────────────────────

    #[tool(
        name = "index_repository",
        description = "**Run this FIRST before using any other tools.** Scan and index an entire codebase using tree-sitter. Extracts symbols (functions, classes, structs, traits, methods), references, imports, and stores them in a local SQLite database for fast querying. Supports 25+ languages including Rust, Python, TypeScript, Go, Java, C/C++, C#, Ruby, PHP, Swift, Kotlin, Scala, and HCL. After indexing, use `get_project_overview` to see what was found, then explore with search and navigation tools."
    )]
    async fn index_repository(
        &self,
        Parameters(args): Parameters<indexing::IndexRepositoryArgs>,
        peer: Peer<RoleServer>,
        meta: Meta,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        indexing::index_repository(&self.state, args, peer, meta).await
    }

    #[tool(
        name = "watch_repository",
        description = "Start watching a repository for file changes and automatically re-index modified files with 800ms debounce. Only one watcher can be active at a time (starting a new one replaces the previous). Call this after `index_repository` to keep the index up-to-date as you edit code. Handles file creation, modification, and deletion events."
    )]
    async fn watch_repository(
        &self,
        Parameters(args): Parameters<indexing::WatchRepositoryArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        indexing::watch_repository(&self.state, args).await
    }

    #[tool(
        name = "stop_watching",
        description = "Stop the active file watcher. Use when you no longer need automatic re-indexing."
    )]
    async fn stop_watching(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        indexing::stop_watching(&self.state).await
    }

    // ── Search tools ────────────────────────────────────────────────

    #[tool(
        name = "search_code",
        description = "Full-text search across all indexed source code using SQLite FTS5. Returns matching file paths, code snippets, and relevance scores. Best for finding code by content keywords. Supports FTS5 query syntax: use AND/OR for boolean logic (e.g. 'error AND handle'), prefix matching with * (e.g. 'auth*'), phrase matching with quotes (e.g. '\"database connection\"'). Use `search_symbols` instead when looking for a specific function or type by name."
    )]
    async fn search_code(
        &self,
        Parameters(args): Parameters<search::SearchCodeArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::search_code(&self.state, args).await
    }

    #[tool(
        name = "search_symbols",
        description = "Search for symbols (functions, classes, structs, traits, interfaces, methods, modules, enums) by name pattern. Faster and more precise than `search_code` when you know the symbol name. Supports filtering by kind (e.g. kind='function', 'class', 'struct', 'trait', 'method', 'module', 'interface', 'enum') and by language. Returns symbol name, kind, file path, line number, scope path, and doc comments. Use `find_definition` afterward to see the full source code."
    )]
    async fn search_symbols(
        &self,
        Parameters(args): Parameters<search::SearchSymbolsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::search_symbols(&self.state, args).await
    }

    #[tool(
        name = "search_by_regex",
        description = "Search source code using a regex pattern (Rust regex syntax). Returns matching lines with file paths. Use for precise pattern matching when FTS5 is not specific enough. Examples: 'fn\\s+\\w+_test' (find test functions), 'TODO|FIXME|HACK' (find annotations), 'impl\\s+\\w+\\s+for' (find trait implementations), 'pub\\s+async\\s+fn' (find public async functions)."
    )]
    async fn search_by_regex(
        &self,
        Parameters(args): Parameters<search::SearchByRegexArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::search_by_regex(&self.state, args).await
    }

    #[tool(
        name = "semantic_search",
        description = "Search code using natural language queries via embeddings (e.g. 'function that handles user authentication'). Uses the AllMiniLM-L6-V2 model for semantic similarity. Requires the 'semantic' feature to be enabled at build time (cargo build --features semantic). Falls back gracefully with an error message if not available. Use when you want concept-based search rather than keyword matching."
    )]
    async fn semantic_search(
        &self,
        Parameters(args): Parameters<search::SemanticSearchArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::semantic_search(&self.state, args).await
    }

    // ── Navigation tools ────────────────────────────────────────────

    #[tool(
        name = "find_definition",
        description = "Find where a symbol is defined. Returns the file path, line number, kind, doc comments, and ±10 lines of surrounding source context with the definition line highlighted. Use after `search_symbols` to see the full source code of a symbol. Provide a `file_hint` to narrow results when the symbol name is common across multiple files."
    )]
    async fn find_definition(
        &self,
        Parameters(args): Parameters<navigate::FindDefinitionArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        navigate::find_definition(&self.state, args).await
    }

    #[tool(
        name = "find_references",
        description = "Find all places where a symbol is referenced across the codebase. Returns file paths, line numbers, reference kind, and ±3 lines of surrounding context for each reference. Essential for understanding a symbol's usage footprint and impact of changes. Provide a `file_hint` to narrow to a specific file."
    )]
    async fn find_references(
        &self,
        Parameters(args): Parameters<navigate::FindReferencesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        navigate::find_references(&self.state, args).await
    }

    #[tool(
        name = "get_imports",
        description = "List all imports and dependencies of a specific file. Returns the import source path and imported names. Use to understand what a file depends on before examining `get_dependency_tree` for the full transitive graph."
    )]
    async fn get_imports(
        &self,
        Parameters(args): Parameters<navigate::GetImportsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        navigate::get_imports(&self.state, args).await
    }

    #[tool(
        name = "get_call_graph",
        description = "Build a caller/callee relationship graph for a symbol. Shows what the symbol calls and what calls it, traversing references within function bodies. Use depth=1 for immediate relationships only, depth=2 (default) for one level of transitive calls, or depth=3-5 for deep analysis. Maximum depth is 5 to prevent excessive traversal."
    )]
    async fn get_call_graph(
        &self,
        Parameters(args): Parameters<graph::GetCallGraphArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        graph::get_call_graph(&self.state, args).await
    }

    #[tool(
        name = "get_dependency_tree",
        description = "Build an import/dependency tree for a file. Set direction='imports' (default) to see what this file depends on, or direction='importers' to see what depends on this file. Controls depth of transitive traversal (default: 3, max: 10). Use direction='importers' to assess the blast radius of changes to a file."
    )]
    async fn get_dependency_tree(
        &self,
        Parameters(args): Parameters<graph::GetDependencyTreeArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        graph::get_dependency_tree(&self.state, args).await
    }

    #[tool(
        name = "get_type_hierarchy",
        description = "Get the type hierarchy for a class, struct, interface, or trait. Shows the type definition, all its members (methods, fields), and implementations/subtypes. Useful for understanding inheritance, trait implementations, and interface conformance in the codebase."
    )]
    async fn get_type_hierarchy(
        &self,
        Parameters(args): Parameters<graph::GetTypeHierarchyArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        graph::get_type_hierarchy(&self.state, args).await
    }

    // ── Context tools ───────────────────────────────────────────────

    #[tool(
        name = "get_file_summary",
        description = "Get a structured summary of a source file: language, size, last indexed time, all imports, and all symbols grouped by kind (Function, Class, Struct, Method, etc.) with line numbers and doc comment excerpts. Use as a quick overview before drilling into specific symbols with `find_definition` or `get_symbol_context`."
    )]
    async fn get_file_summary(
        &self,
        Parameters(args): Parameters<context::GetFileSummaryArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_file_summary(&self.state, args).await
    }

    #[tool(
        name = "get_symbol_context",
        description = "Get comprehensive context for a symbol: its definition with full source code (configurable context_lines, default 15), doc comments, and all references across the codebase grouped by file. This is the most complete view of a symbol — combines what `find_definition` and `find_references` provide in one call. Essential for code review and understanding how a symbol is used throughout the project."
    )]
    async fn get_symbol_context(
        &self,
        Parameters(args): Parameters<context::GetSymbolContextArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_symbol_context(&self.state, args).await
    }

    #[tool(
        name = "get_project_overview",
        description = "Get a high-level overview of the indexed project: total files, total symbols, total references, total size in KB, and a breakdown by language showing file count per language. Call this after `index_repository` to understand the project scope and verify indexing completed successfully."
    )]
    async fn get_project_overview(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_project_overview(&self.state).await
    }

    #[tool(
        name = "get_file_changes",
        description = "Check the indexing status of a specific file: language, size, last indexed time, and whether file content is stored in the database. Use to verify a file is indexed before querying it, or to check if re-indexing is needed after changes."
    )]
    async fn get_file_changes(
        &self,
        Parameters(args): Parameters<context::GetFileChangesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_file_changes(&self.state, args).await
    }
}

// ── Prompt handlers ─────────────────────────────────────────────────

#[prompt_router]
impl CodeContextServer {
    #[prompt(
        name = "onboard_repository",
        description = "Step-by-step guide to index and explore a new codebase. Start here when connecting to a new project. Indexes the repository, shows project overview, identifies key modules, and maps dependencies."
    )]
    async fn onboard_repository(
        &self,
        Parameters(args): Parameters<prompts::OnboardRepositoryArgs>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        Ok(prompts::onboard_repository(args))
    }

    #[prompt(
        name = "explore_codebase",
        description = "Multi-step investigation strategy to answer a question about the codebase. Uses keyword search, symbol search, definition lookup, context analysis, call graphs, and dependency tracing to build a comprehensive answer."
    )]
    async fn explore_codebase(
        &self,
        Parameters(args): Parameters<prompts::ExploreCodebaseArgs>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        Ok(prompts::explore_codebase(args))
    }

    #[prompt(
        name = "understand_symbol",
        description = "Deep-dive analysis of a specific symbol (function, class, struct, trait). Finds definition, gets full context with source code, traces all references, builds call graph, and checks type hierarchy."
    )]
    async fn understand_symbol(
        &self,
        Parameters(args): Parameters<prompts::UnderstandSymbolArgs>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        Ok(prompts::understand_symbol(args))
    }

    #[prompt(
        name = "trace_dependency",
        description = "Analyze dependency relationships for a file. Shows file summary, direct imports, full import tree (outward), reverse dependencies (what depends on it), and key type relationships."
    )]
    async fn trace_dependency(
        &self,
        Parameters(args): Parameters<prompts::TraceDependencyArgs>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        Ok(prompts::trace_dependency(args))
    }

    #[prompt(
        name = "review_changes",
        description = "Impact analysis and code review for a specific file. Shows all symbols, finds all external references to them, maps reverse dependencies, and builds call graphs to assess the blast radius of changes."
    )]
    async fn review_changes(
        &self,
        Parameters(args): Parameters<prompts::ReviewChangesArgs>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        Ok(prompts::review_changes(args))
    }

    #[prompt(
        name = "find_usage_patterns",
        description = "Analyze how a symbol is used across the codebase. Finds all references, groups by file, examines top consumers, and identifies common calling patterns and potential inconsistencies."
    )]
    async fn find_usage_patterns(
        &self,
        Parameters(args): Parameters<prompts::FindUsagePatternsArgs>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        Ok(prompts::find_usage_patterns(args))
    }
}

// ── Server metadata ─────────────────────────────────────────────────

const SERVER_INSTRUCTIONS: &str = r#"# Code Context MCP Server

A high-performance code intelligence server that provides deep codebase understanding via tree-sitter parsing and SQLite indexing. Supports 25+ programming languages.

## Getting Started

**You must index a repository before using any other tools.** Follow this workflow:

1. Call `index_repository` with the absolute path to the repository root
2. Call `get_project_overview` to verify indexing and see the project scope
3. Optionally call `watch_repository` to keep the index updated as files change

## Tool Selection Guide

### When to search for code:
- **Know the symbol name?** → Use `search_symbols` (fastest, most precise)
- **Searching by content/keywords?** → Use `search_code` with FTS5 syntax (AND, OR, prefix*)
- **Need precise patterns?** → Use `search_by_regex` (e.g. 'impl\s+\w+\s+for', 'TODO|FIXME')
- **Have a natural language question?** → Use `semantic_search` (requires semantic feature)

### When to navigate code:
- **Find where something is defined?** → Use `find_definition` (shows source context)
- **Find where something is used?** → Use `find_references` (shows usage locations)
- **See what a file imports?** → Use `get_imports` (direct dependencies only)

### When to analyze relationships:
- **Who calls what?** → Use `get_call_graph` with desired depth (1-5)
- **File dependencies?** → Use `get_dependency_tree` with direction='imports' or 'importers'
- **Type relationships?** → Use `get_type_hierarchy` for classes, structs, traits, interfaces

### When to get context:
- **Quick file overview?** → Use `get_file_summary` (symbols, imports, structure)
- **Complete symbol info?** → Use `get_symbol_context` (definition + all references in one call)
- **Project stats?** → Use `get_project_overview` (files, symbols, languages)
- **File status?** → Use `get_file_changes` (check if indexed, when last updated)

## Best Practices

- Start broad (search_symbols, search_code), then drill down (find_definition, get_symbol_context)
- Use `get_symbol_context` over separate `find_definition` + `find_references` calls when you need both
- For code review, use direction='importers' in `get_dependency_tree` to find the blast radius
- Set appropriate limits to avoid overwhelming results (default: 20 for search, 30 for references)
- Use `file_hint` parameter in `find_definition` and `find_references` to narrow results when symbol names are ambiguous

## Available Prompts

Use prompts for guided multi-step workflows:
- `onboard_repository` — First-time project exploration
- `explore_codebase` — Answer questions about code
- `understand_symbol` — Deep-dive into a specific symbol
- `trace_dependency` — Analyze file dependencies
- `review_changes` — Impact analysis for code review
- `find_usage_patterns` — Discover how APIs are used
"#;

#[tool_handler]
#[prompt_handler]
impl ServerHandler for CodeContextServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(
            ServerCapabilities::builder()
                .enable_tools()
                .enable_prompts()
                .enable_resources()
                .enable_completions()
                .build(),
        )
        .with_instructions(SERVER_INSTRUCTIONS)
        .with_server_info(rmcp::model::Implementation::new(
            "code-context",
            env!("CARGO_PKG_VERSION"),
        ))
    }

    // ── Resources ────────────────────────────────────────────────────────────

    fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourcesResult, rmcp::ErrorData>> + Send + '_
    {
        let resources = resources::static_resources();
        async move {
            Ok(ListResourcesResult {
                resources,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListResourceTemplatesResult, rmcp::ErrorData>>
    + Send
    + '_ {
        let resource_templates = resources::resource_templates();
        async move {
            Ok(ListResourceTemplatesResult {
                resource_templates,
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + '_
    {
        let db = std::sync::Arc::clone(&self.state.db);
        async move { resources::read_resource(&db, request) }
    }

    // ── Completions ──────────────────────────────────────────────────────────

    fn complete(
        &self,
        request: CompleteRequestParams,
        _context: rmcp::service::RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CompleteResult, rmcp::ErrorData>> + Send + '_
    {
        let db = std::sync::Arc::clone(&self.state.db);
        async move {
            let prefix = request.argument.value.as_str();
            let arg_name = request.argument.name.as_str();

            // Determine completion kind from the argument name
            let values: Vec<String> = match arg_name {
                // Symbol name completions
                "symbol_name" | "name" => db
                    .with_conn(|conn| {
                        crate::db::queries::symbol_names_by_prefix(conn, prefix, 20)
                            .map_err(Into::into)
                    })
                    .unwrap_or_default(),

                // File path completions
                "path" | "file_hint" | "file_path" => db
                    .with_conn(|conn| {
                        crate::db::queries::file_paths_by_prefix(conn, prefix, 20)
                            .map_err(Into::into)
                    })
                    .unwrap_or_default(),

                // Static language completions
                "language" => crate::indexer::languages::LanguageRegistry::static_language_names()
                    .iter()
                    .filter(|l| l.starts_with(prefix))
                    .map(|s| s.to_string())
                    .collect(),

                _ => vec![],
            };

            let completion = CompletionInfo::with_all_values(values).unwrap_or_default();
            Ok(CompleteResult::new(completion))
        }
    }
}
