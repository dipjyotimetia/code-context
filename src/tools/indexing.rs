use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{CallToolResult, Meta, ProgressNotificationParam};
use rmcp::{Peer, RoleServer};
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
    peer: Peer<RoleServer>,
    meta: Meta,
) -> Result<CallToolResult, rmcp::ErrorData> {
    let root = PathBuf::from(&args.path);

    // Canonicalize the path to resolve symlinks and prevent traversal
    let root = root.canonicalize().map_err(|e| {
        rmcp::ErrorData::invalid_params(format!("Cannot resolve path '{}': {e}", args.path), None)
    })?;

    if !root.is_dir() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("Not a directory: {}", root.display()),
            None,
        ));
    }

    let db = Arc::clone(&state.db);
    let registry = Arc::clone(&state.registry);

    // If the client supplied a progress token, set up a channel so the
    // blocking indexing thread can send progress updates without calling
    // block_on() from within spawn_blocking (which would panic).
    let on_progress: Option<Box<dyn Fn(usize, usize) + Send>>;
    if let Some(token) = meta.get_progress_token() {
        let (tx, mut rx) = tokio::sync::mpsc::channel::<(usize, usize)>(64);

        // Async forwarder: reads from the channel and sends MCP progress
        // notifications.  Terminates when the tx side drops (end of indexing).
        tokio::spawn(async move {
            while let Some((done, total)) = rx.recv().await {
                let params = ProgressNotificationParam::new(token.clone(), done as f64)
                    .with_total(total as f64)
                    .with_message(format!("indexed {done}/{total} files"));
                if let Err(e) = peer.notify_progress(params).await {
                    tracing::debug!("progress notification error (non-fatal): {e}");
                }
            }
        });

        on_progress = Some(Box::new(move |done: usize, total: usize| {
            // Non-blocking send — skip the notification if the channel is full
            // rather than stalling the indexing thread.
            let _ = tx.try_send((done, total));
        }));
    } else {
        on_progress = None;
    }

    let result = tokio::task::spawn_blocking(move || {
        indexer::index_repository(&root, &db, &registry, on_progress.as_deref())
    })
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

    // Canonicalize the path to resolve symlinks and prevent traversal
    let root = root.canonicalize().map_err(|e| {
        rmcp::ErrorData::invalid_params(format!("Cannot resolve path '{}': {e}", args.path), None)
    })?;

    if !root.is_dir() {
        return Err(rmcp::ErrorData::invalid_params(
            format!("Not a directory: {}", root.display()),
            None,
        ));
    }

    let mut watcher_guard = state.watcher.lock().await;

    // Stop existing watcher if any
    if let Some(ref mut w) = *watcher_guard {
        w.stop().await;
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
        w.stop().await;
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
