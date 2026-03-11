# Architecture

This document describes how the current `code-context` server is put together: process startup, request handling, indexing, storage, file watching, and the feature-gated semantic-search path.

## System overview

At a high level, the server combines:

- **Axum** for the HTTP service
- **rmcp** for MCP server behavior and tool routing
- **tree-sitter** for parsing and code structure extraction
- **SQLite** for persistent indexing and search
- **notify-debouncer-full** for incremental re-indexing

The entry point is [`src/main.rs`](../src/main.rs). It initializes shared state, mounts an rmcp streamable HTTP service at `/mcp`, and serves it through Axum.

## Runtime architecture

```text
MCP client
   |
   v
axum HTTP server
   |
   v
rmcp StreamableHttpService (/mcp)
   |
   v
CodeContextServer
   |
   +--> tools/indexing.rs
   +--> tools/search.rs
   +--> tools/navigate.rs
   +--> tools/graph.rs
   +--> tools/context.rs
            |
            v
         AppState
            |
            +--> Database
            +--> LanguageRegistry
            +--> FileWatcher (optional, mutable)
            +--> SemanticEngine (optional, feature-gated)
```

### Startup sequence

`main` performs the following steps:

1. Initializes tracing with `tracing-subscriber`
2. Reads `HOST`, `PORT`, and `DATABASE_PATH`
3. Opens or creates the SQLite database and initializes schema
4. Builds the tree-sitter language registry
5. Optionally initializes the semantic engine when the `semantic` feature is enabled
6. Creates an rmcp `StreamableHttpService`
7. Mounts that service under `/mcp` in an Axum router
8. Binds a `TcpListener` and serves until shutdown

The current HTTP server configuration uses:

- `stateful_mode: true`
- `json_response: false`
- `sse_keep_alive: None`

Graceful shutdown is handled with a `CancellationToken` and `ctrl-c` signal handling.

## Major modules

### `src/main.rs`

Bootstraps the process, configures logging, creates shared state, and starts the HTTP server.

### `src/server.rs`

Defines `CodeContextServer`, registers MCP tools with rmcp macros, and advertises tool capability metadata.

### `src/state.rs`

Defines `AppState`, the shared runtime container:

- `db: Arc<Database>`
- `registry: Arc<LanguageRegistry>`
- `watcher: Arc<tokio::sync::Mutex<Option<FileWatcher>>>`
- `semantic: Arc<Option<SemanticEngine>>` when the `semantic` feature is enabled

This keeps long-lived resources in one place and makes them cheap to clone into request handlers.

### `src/db`

Contains the SQLite layer:

- `schema.rs` initializes tables, indexes, FTS, and pragmas
- `queries.rs` contains the CRUD and search operations used by tools and the indexer
- `mod.rs` wraps a single `rusqlite::Connection` in a `Mutex`

The current database design uses one SQLite connection guarded by a mutex. That means database access is serialized inside the process, even though tool handlers may run concurrently.

### `src/indexer`

Contains repository scanning and per-file indexing logic:

- `walker.rs` discovers candidate files
- `languages.rs` defines the language registry and extension mapping
- `parser.rs` extracts definitions, references, imports, and doc comments
- `graph.rs` computes scope paths from AST nesting
- `mod.rs` coordinates repository and single-file indexing

### `src/tools`

Implements MCP tool handlers grouped by concern:

- `indexing.rs`
- `search.rs`
- `navigate.rs`
- `graph.rs`
- `context.rs`

Most tool handlers do lightweight validation, then offload blocking work with `tokio::task::spawn_blocking`.

### `src/watcher`

Owns the recursive file watcher and incremental re-index loop.

### `src/semantic`

Provides the optional embedding model and similarity search implementation behind the `semantic` Cargo feature.

## Request flow

For a typical MCP tool call, the runtime path is:

1. **HTTP request arrives** at the Axum server under `/mcp`
2. **rmcp dispatches** the request to the registered tool on `CodeContextServer`
3. **Tool handler validates arguments**
4. **Blocking work is offloaded** with `spawn_blocking` where needed
5. **Database and/or indexer code runs**
6. **Result is formatted** into `CallToolResult`
7. **rmcp returns** the MCP tool response

### Why `spawn_blocking` is used

Two important parts of the system are synchronous:

- `rusqlite` database operations
- tree-sitter parsing and repository walking

The handlers push those operations onto blocking worker threads so the Tokio async runtime is not stalled by CPU-heavy or synchronous I/O work.

## Indexing pipeline

The indexing path is implemented primarily in [`src/indexer/mod.rs`](../src/indexer/mod.rs).

### 1. Repository discovery

`walk_repository` uses `ignore::WalkBuilder` and:

- respects `.gitignore`, global gitignore, and git exclude rules
- does not follow symlinks
- caps traversal depth at 50
- skips files larger than 1 MB
- skips likely binary files by checking for NUL bytes in the first 8 KB
- keeps only files whose extension maps to a registered language

The output is a sorted list of candidate files.

### 2. Batch processing

`index_repository` processes files in batches of 100.

Each batch runs inside a database transaction via `Database::with_tx`. This gives a useful middle ground:

- better performance than committing every file separately
- partial progress is preserved if a later batch fails

### 3. Per-file indexing

For each file:

1. Convert the path to a repository-relative path
2. Detect language from extension
3. Read the file as UTF-8 text
4. Compute a SHA-256 content hash
5. Compare with the stored hash and skip unchanged content
6. If changed, delete prior rows for that file
7. Upsert the `files` record
8. Store raw content in `files_content`
9. Parse the file with tree-sitter
10. Extract definitions, references, and imports
11. Compute scope paths from AST nesting
12. Insert symbols, refs, and imports in batches
13. Upsert an FTS row in `code_fts`

The indexer tracks counts for:

- indexed files
- skipped files
- symbols found
- references found
- indexing errors

### Query-based vs generic parsing

`parser.rs` first tries language-specific tree-sitter queries from the registry. If no query is available for that language, it falls back to generic AST pattern matching for common constructs such as functions, classes, methods, structs, enums, interfaces, modules, and namespaces.

That fallback keeps indexing useful across more formats, but the highest-fidelity extraction comes from the dedicated query-backed languages.

### Scope-path enrichment

After extraction, `indexer/graph.rs` walks ancestor nodes to compute a dot-separated `scope_path` such as:

```text
MyType.my_method
```

That metadata is stored alongside symbol rows and used by symbol-search and summary outputs.

## SQLite schema

The schema is initialized in [`src/db/schema.rs`](../src/db/schema.rs).

### Connection pragmas

The server enables:

- `PRAGMA journal_mode = WAL`
- `PRAGMA foreign_keys = ON`
- `PRAGMA busy_timeout = 5000`

These settings improve concurrent read/write behavior, preserve referential integrity, and reduce transient lock failures.

### Tables

| Table | Purpose |
|---|---|
| `schema_version` | Tracks schema version metadata |
| `files` | One row per indexed file, including path, hash, language, size, and indexed time |
| `symbols` | Definitions extracted from source files |
| `refs` | Symbol references found in source files |
| `imports` | Import/dependency edges at the file level |
| `files_content` | Stored raw file content for context, summaries, regex search, and snippets |
| `embeddings` | Semantic-search chunks and embedding vectors |
| `code_fts` | FTS5 table for full-text code search |

### Relationships

```text
files
 ├── symbols
 ├── refs
 ├── imports
 ├── files_content
 └── embeddings
```

Most child tables use foreign keys back to `files(id)` with `ON DELETE CASCADE`, so removing a file record clears its dependent indexing data automatically.

### Search-specific structures

#### `code_fts`

The FTS5 table stores:

- `symbol_names`
- `content`

Search results join back to `files` by `rowid = files.id` to recover file paths and languages.

#### `embeddings`

The embeddings table stores:

- owning `file_id`
- `chunk_text`
- `chunk_start`
- `chunk_end`
- raw embedding bytes in `embedding`

This table exists in the schema regardless of whether the semantic feature is enabled.

## Watcher behavior

The watcher is implemented in [`src/watcher/mod.rs`](../src/watcher/mod.rs).

### Behavior

- Uses `notify-debouncer-full`
- Debounces events for **800 ms**
- Watches the repository **recursively**
- Filters out `Access` events to avoid re-indexing on reads
- Bridges debounced callbacks into Tokio via an unbounded channel
- Re-indexes only files in supported languages
- Removes deleted files from the index

### Lifecycle

`watch_repository` stores a single watcher in `AppState`.

Important current behavior:

- only one watcher can be active at a time
- starting a new watcher stops the previous one
- `stop_watching` cancels and drops the active watcher

### Re-index path

When the watcher receives a file change:

1. If the path no longer exists, remove its rows from SQLite
2. If the file extension is unsupported, ignore it
3. Otherwise call `index_single_file`

`index_single_file` still hashes the file and skips writes if content has not changed, so watcher-triggered updates remain incremental.

## Semantic-search feature flag

Semantic search is compiled behind the Cargo feature:

```bash
--features semantic
```

### What changes when enabled

- `src/semantic` is compiled in
- `AppState` includes an optional semantic engine handle
- startup attempts to initialize `SemanticEngine`
- the `semantic_search` tool can execute embedding-based similarity search

### Current semantic engine

The implementation uses `fastembed` with:

- model: `AllMiniLML6V2`

`SemanticEngine::search`:

1. embeds the query
2. loads stored vectors from the `embeddings` table
3. computes cosine similarity in-process
4. sorts results by descending score
5. returns the top `limit`

### Important current boundary

The semantic module includes `embed_and_store`, but semantic embedding generation is not wired into the normal repository indexing path in the current code. In other words:

- the feature-gated engine and search path exist
- the schema supports stored embeddings
- embedding population is a clear integration point rather than part of the default indexing flow

That distinction matters when reasoning about why `semantic_search` may be available but still have no indexed vectors to search.

## Extension points

The codebase has a few clear places for extension.

### Add or improve language support

Primary files:

- `src/indexer/languages.rs`
- `languages/*.scm`
- `src/indexer/parser.rs`

Typical steps:

1. add a tree-sitter grammar dependency in `Cargo.toml`
2. register the language and extensions in `LanguageRegistry`
3. add a query file in `languages/` for higher-quality extraction
4. refine generic parsing only if query-based extraction is not enough

### Add a new MCP tool

Primary files:

- `src/tools/*.rs`
- `src/server.rs`

Typical steps:

1. add an argument type with `serde` + `schemars`
2. implement the handler
3. register it on `CodeContextServer` with `#[tool(...)]`
4. use `spawn_blocking` for synchronous database or indexing work

### Extend the schema

Primary files:

- `src/db/schema.rs`
- `src/db/queries.rs`

If you add new persisted relationships or metadata, update both schema creation and the query layer together.

### Improve semantic indexing

Primary files:

- `src/semantic/mod.rs`
- `src/indexer/mod.rs`

The most obvious next step is to call semantic chunking and `embed_and_store` during indexing so semantic search stays in sync with the repository index.

## Current operational characteristics

These are helpful to keep in mind when modifying the system:

- Database access is centralized through one `rusqlite::Connection`
- File paths are stored relative to the indexed repository root
- `find_definition`, `find_references`, and related tools query the persisted index rather than live files
- Full-text search is backed by SQLite FTS5, while regex search scans stored file content in process
- Project overview and file-summary tools are read-only views over indexed data

## Related files

- [`../README.md`](../README.md)
- [`../src/main.rs`](../src/main.rs)
- [`../src/server.rs`](../src/server.rs)
- [`../src/db/schema.rs`](../src/db/schema.rs)
- [`../src/indexer/mod.rs`](../src/indexer/mod.rs)
- [`../src/watcher/mod.rs`](../src/watcher/mod.rs)
