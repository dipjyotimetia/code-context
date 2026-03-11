use rusqlite::Connection;

/// Called by r2d2 on every new connection from the pool.
/// Sets the per-connection PRAGMAs required for correctness and performance.
pub fn configure_connection(conn: &mut Connection) -> rusqlite::Result<()> {
    conn.execute_batch("PRAGMA journal_mode = WAL;")?;
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    conn.execute_batch("PRAGMA busy_timeout = 5000;")?;
    Ok(())
}

pub fn init_schema(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS schema_version (
            version INTEGER NOT NULL,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        INSERT OR IGNORE INTO schema_version (rowid, version) VALUES (1, 1);

        CREATE TABLE IF NOT EXISTS files (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            path        TEXT    NOT NULL UNIQUE,
            content_hash TEXT   NOT NULL,
            language    TEXT    NOT NULL,
            size_bytes  INTEGER NOT NULL DEFAULT 0,
            indexed_at  TEXT    NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS symbols (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            name        TEXT    NOT NULL,
            kind        TEXT    NOT NULL,
            start_line  INTEGER NOT NULL,
            start_col   INTEGER NOT NULL,
            end_line    INTEGER NOT NULL,
            end_col     INTEGER NOT NULL,
            parent_id   INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
            scope_path  TEXT,
            doc_comment TEXT
        );

        CREATE TABLE IF NOT EXISTS refs (
            id                  INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id             INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            symbol_name         TEXT    NOT NULL,
            kind                TEXT    NOT NULL DEFAULT 'usage',
            start_line          INTEGER NOT NULL,
            start_col           INTEGER NOT NULL,
            resolved_symbol_id  INTEGER REFERENCES symbols(id) ON DELETE SET NULL
        );

        CREATE TABLE IF NOT EXISTS imports (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id         INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            source_path     TEXT    NOT NULL,
            imported_names  TEXT
        );

        CREATE TABLE IF NOT EXISTS files_content (
            file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
            content TEXT    NOT NULL
        );

        CREATE TABLE IF NOT EXISTS embeddings (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            file_id     INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
            chunk_text  TEXT    NOT NULL,
            chunk_start INTEGER NOT NULL DEFAULT 0,
            chunk_end   INTEGER NOT NULL DEFAULT 0,
            embedding   BLOB    NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_symbols_name    ON symbols(name);
        CREATE INDEX IF NOT EXISTS idx_symbols_file    ON symbols(file_id);
        CREATE INDEX IF NOT EXISTS idx_symbols_kind    ON symbols(kind);
        CREATE INDEX IF NOT EXISTS idx_refs_name       ON refs(symbol_name);
        CREATE INDEX IF NOT EXISTS idx_refs_file       ON refs(file_id);
        CREATE INDEX IF NOT EXISTS idx_imports_file    ON imports(file_id);
        CREATE INDEX IF NOT EXISTS idx_embeddings_file ON embeddings(file_id);
        CREATE INDEX IF NOT EXISTS idx_files_lang      ON files(language);

        -- Composite indexes for common query patterns
        CREATE INDEX IF NOT EXISTS idx_symbols_file_name   ON symbols(file_id, name);
        CREATE INDEX IF NOT EXISTS idx_refs_name_file      ON refs(symbol_name, file_id);
        CREATE INDEX IF NOT EXISTS idx_symbols_file_line   ON symbols(file_id, start_line);
        CREATE INDEX IF NOT EXISTS idx_files_hash          ON files(content_hash);
        ",
    )?;

    // FTS5 virtual table for full-text search over code content
    conn.execute_batch(
        "
        CREATE VIRTUAL TABLE IF NOT EXISTS code_fts USING fts5(
            symbol_names,
            content,
            content='',
            contentless_delete=1
        );
        ",
    )?;

    // Trigger: keep the FTS index in sync when a file row is deleted.
    // This fires even on cascade-deletes triggered from other tables and
    // protects against orphaned FTS entries if application-level cleanup
    // is skipped (e.g., after a crash mid-transaction).
    conn.execute_batch(
        "
        CREATE TRIGGER IF NOT EXISTS fts_files_delete
        AFTER DELETE ON files
        BEGIN
            DELETE FROM code_fts WHERE rowid = OLD.id;
        END;
        ",
    )?;

    Ok(())
}
