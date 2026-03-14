use std::sync::Arc;

use rmcp::{
    ErrorData as McpError,
    model::{
        Annotated, RawResource, RawResourceTemplate, ReadResourceRequestParams, ReadResourceResult,
        Resource, ResourceContents, ResourceTemplate,
    },
};

use crate::{db::Database, db::queries};

// ── URI constants ────────────────────────────────────────────────────────────

pub const OVERVIEW_URI: &str = "code-context://project/overview";
pub const FILE_TEMPLATE: &str = "code-context://file/{path}";
pub const SYMBOL_TEMPLATE: &str = "code-context://symbol/{name}";

// ── Static resource list ─────────────────────────────────────────────────────

pub fn static_resources() -> Vec<Resource> {
    vec![Annotated {
        raw: RawResource {
            uri: OVERVIEW_URI.to_string(),
            name: "Project Overview".to_string(),
            title: Some("Code-Context Project Overview".to_string()),
            description: Some(
                "High-level statistics about the indexed project: file count, symbol count, \
                 reference count, size, and a language breakdown."
                    .to_string(),
            ),
            mime_type: Some("text/plain".to_string()),
            size: None,
            icons: None,
            meta: None,
        },
        annotations: None,
    }]
}

// ── Resource templates ───────────────────────────────────────────────────────

pub fn resource_templates() -> Vec<ResourceTemplate> {
    vec![
        Annotated {
            raw: RawResourceTemplate {
                uri_template: FILE_TEMPLATE.to_string(),
                name: "Indexed File Content".to_string(),
                title: Some("Source file content by path".to_string()),
                description: Some(
                    "Read the raw source code stored for an indexed file. \
                     Replace {path} with the relative file path (e.g. src/main.rs)."
                        .to_string(),
                ),
                mime_type: Some("text/plain".to_string()),
                icons: None,
            },
            annotations: None,
        },
        Annotated {
            raw: RawResourceTemplate {
                uri_template: SYMBOL_TEMPLATE.to_string(),
                name: "Symbol Definition".to_string(),
                title: Some("Symbol definition and all references".to_string()),
                description: Some(
                    "Read the definition locations and reference list for a named symbol. \
                     Replace {name} with the exact symbol name."
                        .to_string(),
                ),
                mime_type: Some("text/plain".to_string()),
                icons: None,
            },
            annotations: None,
        },
    ]
}

// ── read_resource dispatcher ─────────────────────────────────────────────────

pub fn read_resource(
    db: &Arc<Database>,
    params: ReadResourceRequestParams,
) -> Result<ReadResourceResult, McpError> {
    let uri = &params.uri;

    if uri == OVERVIEW_URI {
        return read_overview(db);
    }

    if let Some(path) = uri.strip_prefix("code-context://file/") {
        return read_file(db, path);
    }

    if let Some(name) = uri.strip_prefix("code-context://symbol/") {
        return read_symbol(db, name);
    }

    Err(McpError::invalid_params(
        format!("unknown resource URI: {uri}"),
        None,
    ))
}

// ── Individual resource readers ──────────────────────────────────────────────

fn read_overview(db: &Arc<Database>) -> Result<ReadResourceResult, McpError> {
    let stats = db
        .with_conn(|conn| queries::get_project_stats(conn).map_err(Into::into))
        .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

    let mut text = format!(
        "Files:     {}\nSymbols:   {}\nRefs:      {}\nSize:      {} KB\n\nLanguages:\n",
        stats.total_files,
        stats.total_symbols,
        stats.total_refs,
        stats.total_size_bytes / 1024,
    );
    for (lang, count) in &stats.languages {
        text.push_str(&format!("  {lang}: {count}\n"));
    }

    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, OVERVIEW_URI).with_mime_type("text/plain"),
    ]))
}

fn read_file(db: &Arc<Database>, path: &str) -> Result<ReadResourceResult, McpError> {
    // Reject path traversal attempts — check for encoded variants too
    if path.contains("..")
        || path.starts_with('/')
        || path.starts_with('\\')
        || path.contains('\0')
    {
        return Err(McpError::invalid_params("path traversal not allowed", None));
    }

    let content = db
        .with_conn(|conn| queries::get_file_content(conn, path).map_err(Into::into))
        .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

    match content {
        Some(text) => {
            let uri = format!("code-context://file/{path}");
            Ok(ReadResourceResult::new(vec![
                ResourceContents::text(text, uri).with_mime_type("text/plain"),
            ]))
        }
        None => Err(McpError::invalid_params(
            format!("file not indexed: {path}"),
            None,
        )),
    }
}

fn read_symbol(db: &Arc<Database>, name: &str) -> Result<ReadResourceResult, McpError> {
    let defs = db
        .with_conn(|conn| queries::find_definitions(conn, name, None).map_err(Into::into))
        .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

    if defs.is_empty() {
        return Err(McpError::invalid_params(
            format!("symbol not found: {name}"),
            None,
        ));
    }

    let refs = db
        .with_conn(|conn| queries::find_references(conn, name, None, 50).map_err(Into::into))
        .map_err(|e| McpError::internal_error(format!("db error: {e}"), None))?;

    let mut text = format!("Symbol: {name}\n\nDefinitions:\n");
    for d in &defs {
        let doc = d
            .doc_comment
            .as_deref()
            .map(|s| format!(" // {}", s.lines().next().unwrap_or("")))
            .unwrap_or_default();
        text.push_str(&format!(
            "  {} ({}) {}:{}-{}{}\n",
            d.file_path, d.kind, d.start_line, d.start_line, d.end_line, doc
        ));
    }

    if !refs.is_empty() {
        text.push_str("\nReferences:\n");
        for r in &refs {
            text.push_str(&format!(
                "  {} :{} ({})\n",
                r.file_path, r.start_line, r.kind
            ));
        }
    }

    let uri = format!("code-context://symbol/{name}");
    Ok(ReadResourceResult::new(vec![
        ResourceContents::text(text, uri).with_mime_type("text/plain"),
    ]))
}
