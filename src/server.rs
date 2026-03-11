use rmcp::{
    ServerHandler,
    handler::server::{router::tool::ToolRouter, wrapper::Parameters},
    model::{CallToolResult, ServerCapabilities, ServerInfo},
    tool, tool_router,
};

use crate::state::AppState;
use crate::tools::{context, graph, indexing, navigate, search};

#[derive(Clone)]
pub struct CodeContextServer {
    state: AppState,
    #[allow(dead_code)] // used by #[tool_router] macro expansion
    tool_router: ToolRouter<Self>,
}

impl CodeContextServer {
    pub fn new(state: AppState) -> Self {
        let tool_router = Self::tool_router();
        Self { state, tool_router }
    }
}

#[tool_router]
impl CodeContextServer {
    // ── Indexing tools ──────────────────────────────────────────────

    #[tool(
        name = "index_repository",
        description = "Scan and index an entire codebase. Extracts symbols, references, imports, and stores them in the local database for fast querying. Run this first on a new project."
    )]
    async fn index_repository(
        &self,
        Parameters(args): Parameters<indexing::IndexRepositoryArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        indexing::index_repository(&self.state, args).await
    }

    #[tool(
        name = "watch_repository",
        description = "Start watching a repository for file changes. Automatically re-indexes modified files with 800ms debounce. Only one watcher can be active at a time."
    )]
    async fn watch_repository(
        &self,
        Parameters(args): Parameters<indexing::WatchRepositoryArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        indexing::watch_repository(&self.state, args).await
    }

    #[tool(name = "stop_watching", description = "Stop the active file watcher.")]
    async fn stop_watching(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        indexing::stop_watching(&self.state).await
    }

    // ── Search tools ────────────────────────────────────────────────

    #[tool(
        name = "search_code",
        description = "Full-text search across all indexed source code using FTS5. Returns matching file paths, snippets, and relevance scores. Use this to find code by content."
    )]
    async fn search_code(
        &self,
        Parameters(args): Parameters<search::SearchCodeArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::search_code(&self.state, args).await
    }

    #[tool(
        name = "search_symbols",
        description = "Search for symbols (functions, classes, structs, etc.) by name pattern. Supports filtering by kind and language."
    )]
    async fn search_symbols(
        &self,
        Parameters(args): Parameters<search::SearchSymbolsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::search_symbols(&self.state, args).await
    }

    #[tool(
        name = "search_by_regex",
        description = "Search source code using a regex pattern. Returns matching lines with file paths. Use for precise pattern matching (e.g. 'fn\\s+\\w+_test', 'TODO|FIXME|HACK')."
    )]
    async fn search_by_regex(
        &self,
        Parameters(args): Parameters<search::SearchByRegexArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        search::search_by_regex(&self.state, args).await
    }

    #[tool(
        name = "semantic_search",
        description = "Search code using natural language queries via embeddings. Requires the 'semantic' feature to be enabled at build time."
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
        description = "Find where a symbol is defined. Returns the file path, line number, kind, doc comments, and surrounding source context."
    )]
    async fn find_definition(
        &self,
        Parameters(args): Parameters<navigate::FindDefinitionArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        navigate::find_definition(&self.state, args).await
    }

    #[tool(
        name = "find_references",
        description = "Find all places where a symbol is referenced across the codebase. Returns file paths and line numbers."
    )]
    async fn find_references(
        &self,
        Parameters(args): Parameters<navigate::FindReferencesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        navigate::find_references(&self.state, args).await
    }

    #[tool(
        name = "get_imports",
        description = "List all imports/dependencies of a specific file."
    )]
    async fn get_imports(
        &self,
        Parameters(args): Parameters<navigate::GetImportsArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        navigate::get_imports(&self.state, args).await
    }

    #[tool(
        name = "get_call_graph",
        description = "Build a call graph for a symbol showing what it calls and what calls it. Traverses references to show caller/callee relationships."
    )]
    async fn get_call_graph(
        &self,
        Parameters(args): Parameters<graph::GetCallGraphArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        graph::get_call_graph(&self.state, args).await
    }

    #[tool(
        name = "get_dependency_tree",
        description = "Build an import/dependency tree for a file showing what it depends on (imports) or what depends on it (importers)."
    )]
    async fn get_dependency_tree(
        &self,
        Parameters(args): Parameters<graph::GetDependencyTreeArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        graph::get_dependency_tree(&self.state, args).await
    }

    #[tool(
        name = "get_type_hierarchy",
        description = "Get the type hierarchy for a class/struct/interface/trait: its definition, members, and implementations/subtypes."
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
        description = "Get a structured summary of a source file: language, size, imports, and all symbols grouped by kind with line numbers and doc comments."
    )]
    async fn get_file_summary(
        &self,
        Parameters(args): Parameters<context::GetFileSummaryArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_file_summary(&self.state, args).await
    }

    #[tool(
        name = "get_symbol_context",
        description = "Get comprehensive context for a symbol: its definition with source code, doc comments, and all references across the codebase. Essential for understanding how a symbol is used."
    )]
    async fn get_symbol_context(
        &self,
        Parameters(args): Parameters<context::GetSymbolContextArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_symbol_context(&self.state, args).await
    }

    #[tool(
        name = "get_project_overview",
        description = "Get a high-level overview of the indexed project: total files, symbols, references, size, and a breakdown by language."
    )]
    async fn get_project_overview(&self) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_project_overview(&self.state).await
    }

    #[tool(
        name = "get_file_changes",
        description = "Check the indexing status of a specific file: language, size, last indexed time, and whether content is stored."
    )]
    async fn get_file_changes(
        &self,
        Parameters(args): Parameters<context::GetFileChangesArgs>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        context::get_file_changes(&self.state, args).await
    }
}

impl ServerHandler for CodeContextServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo::new(ServerCapabilities::builder().enable_tools().build()).with_instructions(
            "Code Context MCP Server — provides deep codebase understanding via \
                 tree-sitter indexing. Index a repository first, then use search, \
                 navigation, and context tools to explore the code.",
        )
    }
}
