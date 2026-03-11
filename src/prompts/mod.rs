use rmcp::model::{GetPromptResult, PromptMessage, PromptMessageRole};
use schemars::JsonSchema;
use serde::Deserialize;

// ── Onboard repository prompt ───────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct OnboardRepositoryArgs {
    #[schemars(description = "Absolute path to the repository root directory")]
    pub path: String,
}

pub fn onboard_repository(args: OnboardRepositoryArgs) -> GetPromptResult {
    let path = &args.path;
    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                r#"I want to understand the codebase at `{path}`. Please follow these steps in order:

1. **Index the repository** — Call `index_repository` with path `{path}` to scan and index all source files. This must happen first before any other tools will work.

2. **Get project overview** — Call `get_project_overview` to see the total files, symbols, references, and language breakdown.

3. **Identify key files** — Use `search_symbols` with kind="module" or kind="class" to find the main entry points and core modules.

4. **Explore architecture** — For each key module, call `get_file_summary` to see its structure (imports, symbols, exports).

5. **Map dependencies** — Call `get_dependency_tree` on the main entry files to understand the import graph.

6. **Start watching** — Call `watch_repository` with path `{path}` so future file changes are automatically re-indexed.

After these steps, summarize the project architecture: what languages it uses, the main modules, entry points, and how they relate to each other."#
            ),
        ),
    ])
    .with_description("Step-by-step guide to onboard and explore a new codebase")
}

// ── Explore codebase prompt ─────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ExploreCodebaseArgs {
    #[schemars(
        description = "Natural language question about the codebase (e.g. 'How does authentication work?')"
    )]
    pub question: String,
}

pub fn explore_codebase(args: ExploreCodebaseArgs) -> GetPromptResult {
    let question = &args.question;
    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                r#"I need to understand: "{question}"

Use the code context tools to investigate systematically:

1. **Keyword search** — Start with `search_code` using key terms from the question. Use FTS5 syntax: AND/OR operators, prefix matching with `*` (e.g. `auth*`).

2. **Symbol search** — Use `search_symbols` to find functions, classes, or structs related to the question. Filter by kind if you know the type (function, class, struct, trait, interface).

3. **Drill into definitions** — For the most relevant symbols found, call `find_definition` to see the full source code with context.

4. **Understand usage** — Call `get_symbol_context` on key symbols to see their definition, documentation, and all references across the codebase.

5. **Trace relationships** — Use `get_call_graph` on critical functions to see what they call and what calls them. Use `get_type_hierarchy` for classes/traits.

6. **Map dependencies** — If the answer involves multiple files, use `get_dependency_tree` to understand how they connect.

Synthesize the findings into a clear answer with code references."#
            ),
        ),
    ])
    .with_description("Multi-step strategy to answer a question about the codebase")
}

// ── Understand symbol prompt ────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct UnderstandSymbolArgs {
    #[schemars(
        description = "The symbol name to deeply understand (function, class, struct, trait, etc.)"
    )]
    pub symbol: String,
}

pub fn understand_symbol(args: UnderstandSymbolArgs) -> GetPromptResult {
    let symbol = &args.symbol;
    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                r#"I want a complete understanding of the symbol `{symbol}`. Perform these steps:

1. **Find definition** — Call `find_definition` with symbol="{symbol}" to locate where it's defined, see its source code, and read its documentation.

2. **Get full context** — Call `get_symbol_context` with symbol="{symbol}" and context_lines=20 to see the definition with extensive surrounding code and all references.

3. **Find all references** — Call `find_references` with symbol="{symbol}" to see every place it's used across the codebase.

4. **Build call graph** — Call `get_call_graph` with symbol="{symbol}" and depth=3 to see what it calls and what calls it.

5. **Check type hierarchy** — If it's a type (class, struct, trait, interface), call `get_type_hierarchy` with type_name="{symbol}" to see implementations, subtypes, and members.

6. **Check imports** — For the file containing the definition, call `get_imports` to understand its dependencies.

Provide a comprehensive summary: what `{symbol}` does, its signature, where it's defined, how it's used, what it depends on, and what depends on it."#
            ),
        ),
    ])
    .with_description(format!("Deep-dive analysis of symbol '{symbol}'"))
}

// ── Trace dependency prompt ─────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct TraceDependencyArgs {
    #[schemars(
        description = "Relative file path to trace dependencies for (e.g. 'src/server.rs')"
    )]
    pub file_path: String,
}

pub fn trace_dependency(args: TraceDependencyArgs) -> GetPromptResult {
    let file_path = &args.file_path;
    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                r#"Analyze the dependency relationships for `{file_path}`:

1. **File summary** — Call `get_file_summary` with path="{file_path}" to see the file's language, size, imports, and all symbols.

2. **Direct imports** — Call `get_imports` with file_path="{file_path}" to list everything this file depends on.

3. **Import tree (outward)** — Call `get_dependency_tree` with file_path="{file_path}", direction="imports", depth=3 to see the full chain of what this file depends on.

4. **Reverse dependencies (inward)** — Call `get_dependency_tree` with file_path="{file_path}", direction="importers", depth=3 to see what other files depend on this one.

5. **Key types** — For major symbols in this file, call `get_type_hierarchy` to see their implementations and relationships.

Summarize: what this file does, what it depends on, what depends on it, and the overall coupling level."#
            ),
        ),
    ])
    .with_description(format!("Dependency analysis for '{file_path}'"))
}

// ── Review changes prompt ───────────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ReviewChangesArgs {
    #[schemars(description = "Relative file path to review (e.g. 'src/handler.rs')")]
    pub file_path: String,
}

pub fn review_changes(args: ReviewChangesArgs) -> GetPromptResult {
    let file_path = &args.file_path;
    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                r#"Review the code in `{file_path}` and assess the impact of changes:

1. **File summary** — Call `get_file_summary` with path="{file_path}" to see all symbols, imports, and structure.

2. **File status** — Call `get_file_changes` with path="{file_path}" to check when it was last indexed and its current state.

3. **Symbol analysis** — For each public function/method/type in the file, call `find_references` to find all callers and consumers across the codebase.

4. **Dependency surface** — Call `get_dependency_tree` with file_path="{file_path}", direction="importers" to find every file that imports this one.

5. **Call chains** — For the most important symbols, call `get_call_graph` with depth=2 to see the immediate blast radius.

Provide a review summary: what the file contains, which symbols are most widely used, what would break if they changed, and the overall risk assessment."#
            ),
        ),
    ])
    .with_description(format!("Impact analysis and review for '{file_path}'"))
}

// ── Find usage patterns prompt ──────────────────────────────────────

#[derive(Debug, Deserialize, JsonSchema)]
pub struct FindUsagePatternsArgs {
    #[schemars(
        description = "Symbol name to find usage patterns for (e.g. 'handleRequest', 'Database')"
    )]
    pub symbol: String,
}

pub fn find_usage_patterns(args: FindUsagePatternsArgs) -> GetPromptResult {
    let symbol = &args.symbol;
    GetPromptResult::new(vec![
        PromptMessage::new_text(
            PromptMessageRole::User,
            format!(
                r#"Analyze how `{symbol}` is used across the codebase:

1. **Find all references** — Call `find_references` with symbol="{symbol}" and limit=50 to find every usage.

2. **Get definition context** — Call `get_symbol_context` with symbol="{symbol}" and context_lines=20 to see the full definition and API surface.

3. **Group by file** — Identify which files use `{symbol}` most heavily based on reference counts.

4. **Examine top consumers** — For the files with the most references, call `get_file_summary` to understand the context of each usage.

5. **Check patterns** — Use `search_by_regex` with patterns around `{symbol}` to find common calling patterns (e.g. error handling patterns, initialization patterns).

Summarize the usage patterns: how many files use `{symbol}`, the most common usage patterns, any anti-patterns or inconsistencies, and whether the API design is clean."#
            ),
        ),
    ])
    .with_description(format!("Usage pattern analysis for '{symbol}'"))
}
