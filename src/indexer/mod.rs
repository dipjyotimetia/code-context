pub mod graph;
pub mod languages;
pub mod parser;
pub mod walker;

use std::path::Path;

use sha2::{Digest, Sha256};
use tracing::{debug, info, instrument, warn};

use crate::db::Database;
use crate::db::queries;
use languages::LanguageRegistry;

#[derive(Debug, Default)]
pub struct IndexResult {
    pub files_indexed: usize,
    pub files_skipped: usize,
    pub symbols_found: usize,
    pub refs_found: usize,
    pub errors: usize,
}

#[instrument(skip(db, registry, on_progress), fields(root = %root.display()))]
pub fn index_repository(
    root: &Path,
    db: &Database,
    registry: &LanguageRegistry,
    on_progress: Option<&(dyn Fn(usize, usize) + Send)>,
) -> anyhow::Result<IndexResult> {
    let files = walker::walk_repository(root, registry);
    let total = files.len();
    info!(total_files = total, root = %root.display(), "starting repository index");

    let mut result = IndexResult::default();

    // Process files in batches to avoid losing all progress on error
    const BATCH_SIZE: usize = 100;
    for chunk in files.chunks(BATCH_SIZE) {
        db.with_tx(|conn| {
            for path in chunk {
                match index_file_inner(conn, path, root, registry) {
                    Ok(IndexFileResult::Indexed { symbols, refs }) => {
                        result.files_indexed += 1;
                        result.symbols_found += symbols;
                        result.refs_found += refs;
                    }
                    Ok(IndexFileResult::Skipped) => {
                        result.files_skipped += 1;
                    }
                    Err(e) => {
                        warn!(path = %path.display(), error = %e, "failed to index file");
                        result.errors += 1;
                    }
                }
            }
            Ok(())
        })?;

        let done = result.files_indexed + result.files_skipped + result.errors;
        debug!(progress = done, total, "indexing progress");
        if let Some(cb) = on_progress {
            cb(done, total);
        }
    }

    info!(
        indexed = result.files_indexed,
        skipped = result.files_skipped,
        symbols = result.symbols_found,
        refs = result.refs_found,
        errors = result.errors,
        "indexing complete"
    );

    Ok(result)
}

pub fn index_single_file(
    path: &Path,
    root: &Path,
    db: &Database,
    registry: &LanguageRegistry,
) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        let rel = make_relative(path, root);

        // Delete old data for this file
        queries::delete_file_by_path(conn, &rel)?;

        // Re-index
        match index_file_inner(conn, path, root, registry) {
            Ok(IndexFileResult::Indexed { symbols, refs }) => {
                debug!(path = %rel, symbols, refs, "re-indexed file");
            }
            Ok(IndexFileResult::Skipped) => {
                debug!(path = %rel, "file skipped (unchanged)");
            }
            Err(e) => {
                warn!(path = %rel, error = %e, "failed to re-index file");
            }
        }

        Ok(())
    })
}

pub fn remove_file(path: &Path, root: &Path, db: &Database) -> anyhow::Result<()> {
    db.with_conn(|conn| {
        let rel = make_relative(path, root);
        queries::delete_file_by_path(conn, &rel)?;
        debug!(path = %rel, "removed file from index");
        Ok(())
    })
}

// ── Internal ────────────────────────────────────────────────────────────────

enum IndexFileResult {
    Indexed { symbols: usize, refs: usize },
    Skipped,
}

fn index_file_inner(
    conn: &rusqlite::Connection,
    path: &Path,
    root: &Path,
    registry: &LanguageRegistry,
) -> anyhow::Result<IndexFileResult> {
    let rel = make_relative(path, root);
    let lang = registry.detect_language(path).unwrap_or("unknown");

    let source = std::fs::read_to_string(path)?;
    let hash = compute_hash(&source);

    // Skip if content unchanged
    if let Some(existing_hash) = queries::get_file_hash(conn, &rel)? {
        if existing_hash == hash {
            return Ok(IndexFileResult::Skipped);
        }
        // Content changed — delete old data
        queries::delete_file_by_path(conn, &rel)?;
    }

    let size_bytes = source.len() as i64;
    let file_id = queries::upsert_file(conn, &rel, &hash, lang, size_bytes)?;

    // Store raw content for context retrieval
    queries::upsert_content(conn, file_id, &source)?;

    // Parse and extract symbols
    let mut parse_result = parser::extract_symbols(&source, lang, registry);

    // Build scope paths via AST analysis
    if let Some(mut parser) = registry.get_parser(lang)
        && let Some(tree) = parser.parse(source.as_bytes(), None)
    {
        graph::build_scope_paths(&source, tree.root_node(), &mut parse_result.definitions);
    }

    // Insert definitions (batch)
    let _symbol_ids = queries::insert_symbols_batch(conn, file_id, &parse_result.definitions)?;

    // Insert references (batch)
    queries::insert_refs_batch(conn, file_id, &parse_result.references)?;

    // Insert imports (batch)
    queries::insert_imports_batch(conn, file_id, &parse_result.imports)?;

    // Build FTS entry
    let symbol_names: Vec<&str> = parse_result
        .definitions
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    let symbol_names_joined = symbol_names.join(" ");
    queries::upsert_fts(conn, file_id, &rel, &symbol_names_joined, &source, lang)?;

    Ok(IndexFileResult::Indexed {
        symbols: parse_result.definitions.len(),
        refs: parse_result.references.len(),
    })
}

fn compute_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn make_relative(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string()
}
