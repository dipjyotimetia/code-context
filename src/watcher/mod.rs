use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use notify::EventKind;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::{debug, info, warn};

use crate::db::Database;
use crate::indexer;
use crate::indexer::languages::LanguageRegistry;

pub struct FileWatcher {
    cancel: CancellationToken,
    handle: Option<tokio::task::JoinHandle<()>>,
    root: PathBuf,
}

impl FileWatcher {
    pub fn start(
        root: PathBuf,
        db: Arc<Database>,
        registry: Arc<LanguageRegistry>,
    ) -> anyhow::Result<Self> {
        let cancel = CancellationToken::new();
        let cancel_clone = cancel.clone();
        let root_clone = root.clone();

        // Bounded channel to bridge notify callbacks into Tokio with backpressure
        let (tx, mut rx) = mpsc::channel::<Vec<PathBuf>>(64);

        // Create the debouncer — runs on its own OS thread
        let mut debouncer = new_debouncer(
            Duration::from_millis(800),
            None,
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        let mut paths = Vec::new();
                        for event in events {
                            // Filter out Access events — they fire on every file
                            // read and would cause unnecessary re-indexing
                            if matches!(event.event.kind, EventKind::Access(_)) {
                                continue;
                            }

                            for path in &event.paths {
                                if path.is_file() {
                                    paths.push(path.clone());
                                }
                            }
                        }
                        if !paths.is_empty() {
                            // Use try_send to avoid blocking the OS notify thread.
                            // Log a warning when the channel is full so that index
                            // staleness is visible rather than silently occurring.
                            if tx.try_send(paths).is_err() {
                                warn!("watcher channel full — file-change events dropped; index may be stale");
                            }
                        }
                    }
                    Err(errors) => {
                        for e in errors {
                            warn!(error = %e, "file watcher debouncer error");
                        }
                    }
                }
            },
        )?;

        // Start watching the root directory recursively
        debouncer.watch(&root, notify::RecursiveMode::Recursive)?;
        info!(root = %root.display(), "started watching for file changes (800ms debounce)");

        // Spawn Tokio task that processes debounced file change events
        let handle = tokio::spawn(async move {
            // Keep debouncer alive for the duration of this task
            let _debouncer = debouncer;

            loop {
                tokio::select! {
                    _ = cancel_clone.cancelled() => {
                        info!("file watcher cancelled, stopping");
                        break;
                    }
                    Some(paths) = rx.recv() => {
                        for path in paths {
                            // Determine if file was removed
                            if !path.exists() {
                                if let Err(e) = indexer::remove_file(&path, &root_clone, &db) {
                                    warn!(path = %path.display(), error = %e, "failed to remove file from index");
                                }
                                continue;
                            }

                            // Check if the file is in a supported language
                            if registry.detect_language(&path).is_none() {
                                continue;
                            }

                            debug!(path = %path.display(), "re-indexing changed file");

                            if let Err(e) = indexer::index_single_file(&path, &root_clone, &db, &registry) {
                                warn!(path = %path.display(), error = %e, "failed to re-index file");
                            }
                        }
                    }
                    else => break,
                }
            }
        });

        Ok(Self {
            cancel,
            handle: Some(handle),
            root,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub async fn stop(&mut self) {
        self.cancel.cancel();
        if let Some(handle) = self.handle.take() {
            // Give the task a chance to finish gracefully before aborting
            match tokio::time::timeout(std::time::Duration::from_secs(5), handle).await {
                Ok(_) => {}
                Err(_) => {
                    warn!(root = %self.root.display(), "watcher task did not stop within timeout");
                }
            }
        }
        info!(root = %self.root.display(), "stopped file watcher");
    }
}

impl Drop for FileWatcher {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use crate::indexer::languages::LanguageRegistry;
    use std::path::PathBuf;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_watcher_start_stop() {
        let db = Arc::new(Database::init(&PathBuf::from(":memory:")).unwrap());
        let registry = Arc::new(LanguageRegistry::new());
        let root = std::env::current_dir().unwrap();

        let mut watcher = FileWatcher::start(root.clone(), db, registry).unwrap();
        assert_eq!(watcher.root(), &root);
        assert!(!watcher.cancel.is_cancelled());

        watcher.stop().await;
        assert!(watcher.cancel.is_cancelled());
    }

    #[tokio::test]
    async fn test_watcher_start_invalid_path() {
        let db = Arc::new(Database::init(&PathBuf::from(":memory:")).unwrap());
        let registry = Arc::new(LanguageRegistry::new());
        let root = PathBuf::from("/does/not/exist/we/hope");

        let watcher_result = FileWatcher::start(root, db, registry);
        assert!(
            watcher_result.is_err(),
            "Watcher should fail on invalid path"
        );
    }
}
