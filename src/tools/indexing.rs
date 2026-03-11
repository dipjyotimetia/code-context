use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::CallToolResult;
use schemars::JsonSchema;
use serde::Deserialize;

use crate::indexer;
use crate::state::AppState;
use crate::watcher::FileWatcher;

#[derive(Debug, Deserialize, JsonSchema)]
pub struct IndexRepositoryArgs {
    #[schemars(description = "Absolute path to the repository root directory to index")]
    pub path: String,
}

pub async fn index_repository(
    state: &AppState,
    args: IndexRepositoryArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let root = PathBuf::from(&args.path);
    if !root.is_dir() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("Not a directory: {}", args.path),
            None,
        ));
    }

    let db = Arc::clone(&state.db);
    let registry = Arc::clone(&state.registry);

    // Run indexing in a blocking task to avoid blocking the Tokio runtime
    let result =
        tokio::task::spawn_blocking(move || indexer::index_repository(&root, &db, &registry))
            .await
            .map_err(|e| rmcp::ErrorData::internal_error(format!("task join error: {e}"), None))?
            .map_err(|e| rmcp::ErrorData::internal_error(format!("indexing error: {e}"), None))?;

    let summary = format!(
        "Indexed {} files ({} skipped), found {} symbols and {} references. {} errors.",
        result.files_indexed,
        result.files_skipped,
        result.symbols_found,
        result.refs_found,
        result.errors,
    );

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        summary,
    )]))
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct WatchRepositoryArgs {
    #[schemars(
        description = "Absolute path to the repository root directory to watch for changes"
    )]
    pub path: String,
}

pub async fn watch_repository(
    state: &AppState,
    args: WatchRepositoryArgs,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let root = PathBuf::from(&args.path);
    if !root.is_dir() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("Not a directory: {}", args.path),
            None,
        ));
    }

    let mut watcher_guard = state.watcher.lock().await;

    // Stop existing watcher if any
    if let Some(ref mut w) = *watcher_guard {
        w.stop();
    }

    let new_watcher = FileWatcher::start(
        root.clone(),
        Arc::clone(&state.db),
        Arc::clone(&state.registry),
    )
    .map_err(|e| rmcp::ErrorData::internal_error(format!("watcher start error: {e}"), None))?;

    *watcher_guard = Some(new_watcher);

    Ok(CallToolResult::success(vec![rmcp::model::Content::text(
        format!("Watching {} for changes (800ms debounce)", root.display()),
    )]))
}

pub async fn stop_watching(state: &AppState) -> Result<CallToolResult, rmcp::ErrorData> {
    let mut watcher_guard = state.watcher.lock().await;

    if let Some(ref mut w) = *watcher_guard {
        let root = w.root().to_string_lossy().to_string();
        w.stop();
        *watcher_guard = None;
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            format!("Stopped watching {}", root),
        )]))
    } else {
        Ok(CallToolResult::success(vec![rmcp::model::Content::text(
            "No active file watcher to stop",
        )]))
    }
}
