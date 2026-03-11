# code-context

> MCP server providing deep codebase context via tree-sitter indexing.

`code-context` is a Rust MCP server that indexes a local repository into SQLite and exposes fast tools for code search, symbol lookup, navigation, and project-level context.

It is designed for MCP clients that need reliable code intelligence without re-parsing a repository on every request.

## Features

- Tree-sitter-based indexing for code structure
- SQLite-backed storage with FTS5 full-text search
- Symbol, reference, and import lookup
- File, symbol, and project overview tools
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

Then use tools such as `get_project_overview`, `search_code`, `search_symbols`, `find_definition`, and `get_symbol_context`.

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

## Development

```bash
make check
make test
make run
```

## Documentation

- [Architecture](./docs/architecture.md)
- [Code of Conduct](./CODE_OF_CONDUCT.md)
- [Security Policy](./SECURITY.md)
- [License](./LICENSE)
