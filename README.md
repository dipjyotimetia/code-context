# code-context

> MCP server providing deep codebase context via tree-sitter indexing.

`code-context` sits between MCP clients and a local repository index. Clients call MCP tools over HTTP, and the server answers from persisted repository data instead of reparsing the codebase on every request.

```mermaid
flowchart LR
    Client[MCP client] -->|HTTP requests| Axum[Axum server]
    Axum -->|/mcp| RMCP[rmcp StreamableHttpService]
    RMCP --> Server[CodeContextServer]

    Server --> State[Shared AppState]
    Server --> Tools[Tool handlers]
    Server --> Prompts[Prompt handlers]
    State --> DB[(SQLite database)]
    State --> Registry[LanguageRegistry]
    State --> Watcher[Optional FileWatcher]
    State --> Semantic[Optional SemanticEngine]

    Server --> Repo[Local repository]
    Watcher --> Repo
    Repo --> Indexer[Indexer]
    Indexer --> DB
```

## Features

- Tree-sitter-based indexing for code structure
- SQLite-backed storage with FTS5 full-text search
- Symbol, reference, and import lookup
- File, symbol, and project overview tools
- Built-in MCP prompts for common exploration workflows
- Incremental re-indexing with file watching
- Optional semantic search via the `semantic` feature

## Quick start

```bash
cargo run
```

By default, the server is available at `http://127.0.0.1:3001/mcp`.

Index a repository:

```text
index_repository({
  "path": "/absolute/path/to/repository"
})
```

Then use tools such as `get_project_overview`, `search_code`, `search_symbols`, `find_definition`, and `get_symbol_context`, or start with a built-in prompt such as `onboard_repository`.

## Supported languages

`code-context` ships with dedicated tree-sitter queries for Bash, C, C++, C#, Go, HCL, Java, Kotlin, PHP, Python, Ruby, Rust, Scala, Swift, and TypeScript.

The language registry also supports additional formats including JavaScript, TSX, JSON, TOML, YAML, HTML, CSS, and Markdown for indexing and search.

## Build and run

```bash
cargo build
cargo run
```

Enable semantic search:

```bash
cargo run --features semantic
```

Useful Make targets:

- `make build`
- `make run`
- `make check`
- `make test`
- `make help`

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `HOST` | `127.0.0.1` | Bind address |
| `PORT` | `3001` | Bind port |
| `DATABASE_PATH` | `code_context.db` | SQLite database path |
| `RUST_LOG` | `info` fallback | Tracing filter |

Example:

```bash
HOST=0.0.0.0 PORT=4000 DATABASE_PATH=.data/code-context.db RUST_LOG=debug cargo run
```

## MCP tools

The server exposes tools for:

- repository indexing and watching
- full-text and symbol search
- definition and reference lookup
- import, call-graph, and dependency views
- file, symbol, and project context

It also exposes guided MCP prompts for common workflows such as onboarding a repository, exploring a codebase question, tracing dependencies, and reviewing change impact.

## Development

```bash
make check
make test
make run
```

## Documentation

- [Architecture](./docs/architecture.md)
- [Design decisions](./docs/design-decisions.md)
- [Why Rust?](./docs/design-decisions.md#decision-1-implement-the-server-in-rust)
- [Code of Conduct](./CODE_OF_CONDUCT.md)
- [Security Policy](./SECURITY.md)
- [License](./LICENSE)
