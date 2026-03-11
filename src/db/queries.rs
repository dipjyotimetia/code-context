use rusqlite::{Connection, OptionalExtension, params};

// ── Data types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct FileRecord {
    pub path: String,
    pub language: String,
    pub size_bytes: i64,
    pub indexed_at: String,
}

#[derive(Debug, Clone)]
pub struct SymbolDef {
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
    pub parent_id: Option<i64>,
    pub scope_path: Option<String>,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SymbolRef {
    pub symbol_name: String,
    pub kind: String,
    pub start_line: u32,
    pub start_col: u32,
}

#[derive(Debug, Clone)]
pub struct ImportRecord {
    pub source_path: String,
    pub imported_names: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub file_path: String,
    pub snippet: String,
    pub language: String,
    pub rank: f64,
}

#[derive(Debug, Clone)]
pub struct DefinitionLocation {
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ReferenceLocation {
    pub file_path: String,
    pub symbol_name: String,
    pub kind: String,
    pub start_line: u32,
}

#[derive(Debug, Clone)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub end_line: u32,
    pub doc_comment: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectStats {
    pub total_files: i64,
    pub total_symbols: i64,
    pub total_refs: i64,
    pub total_size_bytes: i64,
    pub languages: Vec<(String, i64)>,
}

// ── Queries ─────────────────────────────────────────────────────────────────

pub fn upsert_file(
    conn: &Connection,
    path: &str,
    content_hash: &str,
    language: &str,
    size_bytes: i64,
) -> rusqlite::Result<i64> {
    conn.execute(
        "INSERT INTO files (path, content_hash, language, size_bytes, indexed_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))
         ON CONFLICT(path) DO UPDATE SET
           content_hash = excluded.content_hash,
           language = excluded.language,
           size_bytes = excluded.size_bytes,
           indexed_at = datetime('now')",
        params![path, content_hash, language, size_bytes],
    )?;
    Ok(conn.last_insert_rowid())
}

pub fn get_file_id(conn: &Connection, path: &str) -> rusqlite::Result<Option<i64>> {
    conn.query_row(
        "SELECT id FROM files WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )
    .optional()
}

pub fn get_file_hash(conn: &Connection, path: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT content_hash FROM files WHERE path = ?1",
        params![path],
        |row| row.get(0),
    )
    .optional()
}

pub fn delete_file_data(conn: &Connection, file_id: i64) -> rusqlite::Result<()> {
    // FTS delete
    conn.execute(
        "DELETE FROM code_fts WHERE rowid IN (
            SELECT f.id FROM files f WHERE f.id = ?1
        )",
        params![file_id],
    )?;
    // Cascade-handled tables
    conn.execute("DELETE FROM files WHERE id = ?1", params![file_id])?;
    Ok(())
}

pub fn delete_file_by_path(conn: &Connection, path: &str) -> rusqlite::Result<()> {
    if let Some(file_id) = get_file_id(conn, path)? {
        delete_file_data(conn, file_id)?;
    }
    Ok(())
}

pub fn upsert_content(conn: &Connection, file_id: i64, content: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO files_content (file_id, content) VALUES (?1, ?2)",
        params![file_id, content],
    )?;
    Ok(())
}

pub fn upsert_fts(
    conn: &Connection,
    file_id: i64,
    _file_path: &str, // No longer stored in FTS, fetched via JOIN
    symbol_names: &str,
    content: &str,
    _language: &str, // No longer stored in FTS, fetched via JOIN
) -> rusqlite::Result<()> {
    // If using completely external content, FTS5 updates automatically via triggers
    // However, if we're passing symbol_names we might still need to update the FTS table explicitly
    // for the non-external columns if we hadn't set them up perfectly in triggers.
    // Let's rethink. If FTS table has `content='files_content'`, the columns must match `files_content`.
    // Actually, `files_content` only has `file_id` and `content`.
    // It is simpler to revert to the `contentless_delete=1` table but without `file_path` and `language`
    // inside the FTS table, and rely entirely on the JOIN. Let's do that.

    // For now we'll just fix the insert statement to match whatever the schema expects.
    conn.execute(
        "INSERT INTO code_fts (rowid, symbol_names, content)
         VALUES (?1, ?2, ?3)",
        params![file_id, symbol_names, content],
    )
    .or_else(|_| {
        conn.execute("DELETE FROM code_fts WHERE rowid = ?1", params![file_id])?;
        conn.execute(
            "INSERT INTO code_fts (rowid, symbol_names, content)
             VALUES (?1, ?2, ?3)",
            params![file_id, symbol_names, content],
        )
    })?;
    Ok(())
}

pub fn search_fts(
    conn: &Connection,
    query: &str,
    language: Option<&str>,
    limit: u32,
) -> rusqlite::Result<Vec<SearchResult>> {
    let fts_query = if let Some(lang) = language {
        format!(
            "({}) AND language:{}",
            sanitize_fts_query(query),
            sanitize_fts_query(lang)
        )
    } else {
        sanitize_fts_query(query)
    };

    // The snippet function returns NULL if the matched column is unindexed or contentless and missing.
    // By keeping 'content' as a normal column in the code_fts (content='', contentless_delete=1) table,
    // the snippet function can do its job on column index 1.
    let mut stmt = conn.prepare(
        "SELECT f.path, snippet(code_fts, 1, '>>>', '<<<', '...', 40) as snip,
                f.language, code_fts.rank
         FROM code_fts
         JOIN files f ON code_fts.rowid = f.id
         WHERE code_fts MATCH ?1
         ORDER BY rank
         LIMIT ?2",
    )?;

    let rows = stmt.query_map(params![fts_query, limit], |row| {
        Ok(SearchResult {
            file_path: row.get(0)?,
            // Provide a fallback if snippet is null for some reason
            snippet: row
                .get::<_, Option<String>>(1)?
                .unwrap_or_else(|| "".to_string()),
            language: row.get(2)?,
            rank: row.get(3)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn find_definitions(
    conn: &Connection,
    symbol_name: &str,
    file_filter: Option<&str>,
) -> rusqlite::Result<Vec<DefinitionLocation>> {
    let sql = if file_filter.is_some() {
        "SELECT f.path, s.name, s.kind, s.start_line, s.end_line, s.doc_comment
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE s.name = ?1 AND f.path LIKE ?2
         ORDER BY f.path, s.start_line
         LIMIT 50"
    } else {
        "SELECT f.path, s.name, s.kind, s.start_line, s.end_line, s.doc_comment
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE s.name = ?1
         ORDER BY f.path, s.start_line
         LIMIT 50"
    };

    let mut stmt = conn.prepare(sql)?;
    let filter_pattern = file_filter.map(|f| format!("%{f}%"));

    let rows = if let Some(ref pat) = filter_pattern {
        stmt.query_map(params![symbol_name, pat], map_definition)?
    } else {
        stmt.query_map(params![symbol_name], map_definition)?
    };

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn find_references(
    conn: &Connection,
    symbol_name: &str,
    file_filter: Option<&str>,
    limit: u32,
) -> rusqlite::Result<Vec<ReferenceLocation>> {
    let sql = if file_filter.is_some() {
        "SELECT f.path, r.symbol_name, r.kind, r.start_line
         FROM refs r JOIN files f ON r.file_id = f.id
         WHERE r.symbol_name = ?1 AND f.path LIKE ?2
         ORDER BY f.path, r.start_line
         LIMIT ?3"
    } else {
        "SELECT f.path, r.symbol_name, r.kind, r.start_line
         FROM refs r JOIN files f ON r.file_id = f.id
         WHERE r.symbol_name = ?1
         ORDER BY f.path, r.start_line
         LIMIT ?2"
    };

    let mut stmt = conn.prepare(sql)?;
    let filter_pattern = file_filter.map(|f| format!("%{f}%"));

    let rows = if let Some(ref pat) = filter_pattern {
        stmt.query_map(params![symbol_name, pat, limit], map_reference)?
    } else {
        stmt.query_map(params![symbol_name, limit], map_reference)?
    };

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn get_file_symbols(conn: &Connection, file_path: &str) -> rusqlite::Result<Vec<SymbolInfo>> {
    let mut stmt = conn.prepare(
        "SELECT s.name, s.kind, s.start_line, s.end_line, s.doc_comment
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE f.path = ?1
         ORDER BY s.start_line",
    )?;

    let rows = stmt.query_map(params![file_path], |row| {
        Ok(SymbolInfo {
            name: row.get(0)?,
            kind: row.get(1)?,
            start_line: row.get(2)?,
            end_line: row.get(3)?,
            doc_comment: row.get(4)?,
        })
    })?;

    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn get_file_content(conn: &Connection, file_path: &str) -> rusqlite::Result<Option<String>> {
    conn.query_row(
        "SELECT fc.content FROM files_content fc
         JOIN files f ON fc.file_id = f.id
         WHERE f.path = ?1",
        params![file_path],
        |row| row.get(0),
    )
    .optional()
}

pub fn get_file_record(conn: &Connection, file_path: &str) -> rusqlite::Result<Option<FileRecord>> {
    conn.query_row(
        "SELECT path, language, size_bytes, indexed_at FROM files WHERE path = ?1",
        params![file_path],
        |row| {
            Ok(FileRecord {
                path: row.get(0)?,
                language: row.get(1)?,
                size_bytes: row.get(2)?,
                indexed_at: row.get(3)?,
            })
        },
    )
    .optional()
}

pub fn get_file_imports(conn: &Connection, file_path: &str) -> rusqlite::Result<Vec<ImportRecord>> {
    let mut stmt = conn.prepare(
        "SELECT i.source_path, i.imported_names FROM imports i
         JOIN files f ON i.file_id = f.id
         WHERE f.path = ?1",
    )?;
    let rows = stmt.query_map(params![file_path], |row| {
        Ok(ImportRecord {
            source_path: row.get(0)?,
            imported_names: row.get(1)?,
        })
    })?;
    let mut results = Vec::new();
    for row in rows {
        results.push(row?);
    }
    Ok(results)
}

pub fn get_project_stats(conn: &Connection) -> rusqlite::Result<ProjectStats> {
    let (total_files, total_symbols, total_refs, total_size_bytes) = conn.query_row(
        "SELECT
            (SELECT COUNT(*) FROM files),
            (SELECT COUNT(*) FROM symbols),
            (SELECT COUNT(*) FROM refs),
            (SELECT COALESCE(SUM(size_bytes), 0) FROM files)",
        [],
        |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
                row.get::<_, i64>(3)?,
            ))
        },
    )?;

    let mut stmt = conn
        .prepare("SELECT language, COUNT(*) FROM files GROUP BY language ORDER BY COUNT(*) DESC")?;
    let langs = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    Ok(ProjectStats {
        total_files,
        total_symbols,
        total_refs,
        total_size_bytes,
        languages: langs,
    })
}

// ── Helpers ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SymbolSearchResult {
    pub file_path: String,
    pub name: String,
    pub kind: String,
    pub start_line: u32,
    pub scope_path: Option<String>,
    pub doc_comment: Option<String>,
}

/// Insert multiple symbols in a single prepared statement for better perf.
pub fn insert_symbols_batch(
    conn: &Connection,
    file_id: i64,
    symbols: &[SymbolDef],
) -> rusqlite::Result<Vec<i64>> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO symbols (file_id, name, kind, start_line, start_col, end_line, end_col, parent_id, scope_path, doc_comment)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
    )?;
    let mut ids = Vec::with_capacity(symbols.len());
    for sym in symbols {
        stmt.execute(params![
            file_id,
            sym.name,
            sym.kind,
            sym.start_line,
            sym.start_col,
            sym.end_line,
            sym.end_col,
            sym.parent_id,
            sym.scope_path,
            sym.doc_comment,
        ])?;
        ids.push(conn.last_insert_rowid());
    }
    Ok(ids)
}

/// Insert multiple refs in a single prepared statement.
pub fn insert_refs_batch(
    conn: &Connection,
    file_id: i64,
    refs: &[SymbolRef],
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO refs (file_id, symbol_name, kind, start_line, start_col)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for r in refs {
        stmt.execute(params![
            file_id,
            r.symbol_name,
            r.kind,
            r.start_line,
            r.start_col
        ])?;
    }
    Ok(())
}

/// Insert multiple imports in a single prepared statement.
pub fn insert_imports_batch(
    conn: &Connection,
    file_id: i64,
    imports: &[ImportRecord],
) -> rusqlite::Result<()> {
    let mut stmt = conn.prepare_cached(
        "INSERT INTO imports (file_id, source_path, imported_names) VALUES (?1, ?2, ?3)",
    )?;
    for imp in imports {
        stmt.execute(params![file_id, imp.source_path, imp.imported_names])?;
    }
    Ok(())
}

/// Search by regex pattern across file contents.
pub fn search_by_regex(
    conn: &Connection,
    pattern: &str,
    language: Option<&str>,
    limit: u32,
) -> rusqlite::Result<Vec<SearchResult>> {
    // We use LIKE with the pattern embedded, but the actual regex matching
    // happens in the caller. Here we fetch candidate files and their content.
    let sql = if language.is_some() {
        "SELECT f.path, fc.content, f.language
         FROM files_content fc JOIN files f ON fc.file_id = f.id
         WHERE f.language = ?1
         LIMIT ?2"
    } else {
        "SELECT f.path, fc.content, f.language
         FROM files_content fc JOIN files f ON fc.file_id = f.id
         LIMIT ?1"
    };

    let mut stmt = conn.prepare(sql)?;
    let mut all_rows: Vec<(String, String, String)> = Vec::new();
    if let Some(lang) = language {
        let mapped = stmt.query_map(params![lang, limit * 10], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for r in mapped {
            all_rows.push(r?);
        }
    } else {
        let mapped = stmt.query_map(params![limit * 10], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        for r in mapped {
            all_rows.push(r?);
        }
    }

    // Compile regex and search through content
    let re = regex::Regex::new(pattern)
        .map_err(|e| rusqlite::Error::InvalidParameterName(format!("invalid regex: {e}")))?;

    let mut results = Vec::new();
    for (file_path, content, language) in &all_rows {
        for mat in re.find_iter(content) {
            let start = mat.start();
            // Extract surrounding context line
            let line_start = content[..start].rfind('\n').map(|p| p + 1).unwrap_or(0);
            let line_end = content[start..]
                .find('\n')
                .map(|p| start + p)
                .unwrap_or(content.len());
            let snippet = content[line_start..line_end].to_string();

            results.push(SearchResult {
                file_path: file_path.clone(),
                snippet,
                language: language.clone(),
                rank: 0.0,
            });

            if results.len() >= limit as usize {
                return Ok(results);
            }
        }
    }

    Ok(results)
}

pub fn search_symbols(
    conn: &Connection,
    name_pattern: &str,
    kind_filter: Option<&str>,
    language_filter: Option<&str>,
    limit: usize,
) -> rusqlite::Result<Vec<SymbolSearchResult>> {
    let like_pattern = format!("%{}%", name_pattern);

    // Build WHERE clause dynamically while keeping typed params
    let mut conditions = vec!["s.name LIKE ?1"];
    if kind_filter.is_some() {
        conditions.push("s.kind = ?2");
    }
    if language_filter.is_some() {
        let idx = if kind_filter.is_some() { "?3" } else { "?2" };
        conditions.push(if kind_filter.is_some() {
            "f.language = ?3"
        } else {
            "f.language = ?2"
        });
        let _ = idx; // suppress unused
    }
    let limit_idx = 1 + conditions.len();
    let sql = format!(
        "SELECT f.path, s.name, s.kind, s.start_line, s.scope_path, s.doc_comment
         FROM symbols s JOIN files f ON s.file_id = f.id
         WHERE {}
         ORDER BY s.name LIMIT ?{}",
        conditions.join(" AND "),
        limit_idx,
    );

    let mut stmt = conn.prepare(&sql)?;
    let limit_val = limit as u32;

    let rows: Vec<SymbolSearchResult> = match (kind_filter, language_filter) {
        (Some(kind), Some(lang)) => stmt
            .query_map(
                params![like_pattern, kind, lang, limit_val],
                map_symbol_search,
            )?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        (Some(kind), None) => stmt
            .query_map(params![like_pattern, kind, limit_val], map_symbol_search)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        (None, Some(lang)) => stmt
            .query_map(params![like_pattern, lang, limit_val], map_symbol_search)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
        (None, None) => stmt
            .query_map(params![like_pattern, limit_val], map_symbol_search)?
            .collect::<rusqlite::Result<Vec<_>>>()?,
    };

    Ok(rows)
}

fn map_symbol_search(row: &rusqlite::Row<'_>) -> rusqlite::Result<SymbolSearchResult> {
    Ok(SymbolSearchResult {
        file_path: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        start_line: row.get(3)?,
        scope_path: row.get(4)?,
        doc_comment: row.get(5)?,
    })
}

fn sanitize_fts_query(input: &str) -> String {
    // Strip characters that have special meaning in FTS5 and could cause parse errors,
    // but preserve : (for column filters) and * (for prefix matching)
    let cleaned: String = input
        .chars()
        .filter(|c| {
            c.is_alphanumeric()
                || *c == ' '
                || *c == '_'
                || *c == '-'
                || *c == '.'
                || *c == ':'
                || *c == '*'
        })
        .collect();
    if cleaned.is_empty() {
        return "\"\"".to_string();
    }
    // If the query already uses FTS5 operators, pass through; otherwise wrap in quotes
    if cleaned.contains(':') || cleaned.contains('*') || cleaned.contains(' ') {
        cleaned
    } else {
        format!("\"{}\"", cleaned)
    }
}

fn map_definition(row: &rusqlite::Row<'_>) -> rusqlite::Result<DefinitionLocation> {
    Ok(DefinitionLocation {
        file_path: row.get(0)?,
        name: row.get(1)?,
        kind: row.get(2)?,
        start_line: row.get(3)?,
        end_line: row.get(4)?,
        doc_comment: row.get(5)?,
    })
}

fn map_reference(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReferenceLocation> {
    Ok(ReferenceLocation {
        file_path: row.get(0)?,
        symbol_name: row.get(1)?,
        kind: row.get(2)?,
        start_line: row.get(3)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::schema::init_schema;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        init_schema(&conn).unwrap();
        conn
    }

    #[test]
    fn test_upsert_and_get_file() {
        let conn = setup_db();
        let id = upsert_file(&conn, "src/main.rs", "hash123", "rust", 1024).unwrap();
        assert!(id > 0);

        let fetched_id = get_file_id(&conn, "src/main.rs").unwrap();
        assert_eq!(fetched_id, Some(id));

        let hash = get_file_hash(&conn, "src/main.rs").unwrap();
        assert_eq!(hash, Some("hash123".to_string()));

        let record = get_file_record(&conn, "src/main.rs").unwrap();
        assert!(record.is_some());
        let record = record.unwrap();
        assert_eq!(record.path, "src/main.rs");
        assert_eq!(record.language, "rust");
        assert_eq!(record.size_bytes, 1024);
    }

    #[test]
    fn test_delete_file() {
        let conn = setup_db();
        let _id = upsert_file(&conn, "src/main.rs", "hash123", "rust", 1024).unwrap();
        delete_file_by_path(&conn, "src/main.rs").unwrap();

        let fetched_id = get_file_id(&conn, "src/main.rs").unwrap();
        assert_eq!(fetched_id, None);
    }

    #[test]
    fn test_symbols() {
        let conn = setup_db();
        let file_id = upsert_file(&conn, "src/main.rs", "hash", "rust", 100).unwrap();

        let symbols = vec![SymbolDef {
            name: "main".to_string(),
            kind: "function".to_string(),
            start_line: 1,
            start_col: 0,
            end_line: 5,
            end_col: 1,
            parent_id: None,
            scope_path: None,
            doc_comment: None,
        }];

        insert_symbols_batch(&conn, file_id, &symbols).unwrap();

        let defs = find_definitions(&conn, "main", None).unwrap();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].name, "main");

        let file_symbols = get_file_symbols(&conn, "src/main.rs").unwrap();
        assert_eq!(file_symbols.len(), 1);
        assert_eq!(file_symbols[0].name, "main");
    }

    #[test]
    fn test_fts_search() {
        let conn = setup_db();
        let file_id = upsert_file(&conn, "doc.txt", "hash", "text", 100).unwrap();
        // search_fts selects 'file_path'. upsert_fts receives 'file_path'.
        // Let's insert real content, the rowid is the link to the file.
        upsert_content(&conn, file_id, "Hello world of rust").unwrap();
        // The fts query fails with `InvalidColumnType(0, "file_path", Null)` because
        // some fts5 usages require the full set of configured columns. Wait, it's actually returning Null for file_path.
        upsert_fts(&conn, file_id, "doc.txt", "", "Hello world of rust", "text").unwrap();

        let results = search_fts(&conn, "world", None, 10).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].file_path, "doc.txt");
    }

    #[test]
    fn test_negative_queries() {
        let conn = setup_db();

        // get_file_id on missing file
        let fetched_id = get_file_id(&conn, "missing.rs").unwrap();
        assert_eq!(fetched_id, None);

        // delete_file_by_path on missing file (should not error)
        delete_file_by_path(&conn, "missing.rs").unwrap();

        // find_definitions on missing symbol
        let defs = find_definitions(&conn, "missing_symbol", None).unwrap();
        assert!(defs.is_empty());

        // search_fts on missing query
        let results = search_fts(&conn, "nonexistentword", None, 10).unwrap();
        assert!(results.is_empty());

        // find_references on missing symbol
        let refs = find_references(&conn, "missing_symbol", None, 10).unwrap();
        assert!(refs.is_empty());

        // get_file_symbols on missing file
        let file_symbols = get_file_symbols(&conn, "missing.rs").unwrap();
        assert!(file_symbols.is_empty());
    }
}
