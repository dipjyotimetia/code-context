use std::sync::Arc;

use tokio::sync::Mutex;

use crate::db::Database;
use crate::indexer::languages::LanguageRegistry;
#[cfg(feature = "semantic")]
use crate::semantic::SemanticEngine;
use crate::watcher::FileWatcher;

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<Database>,
    pub registry: Arc<LanguageRegistry>,
    pub watcher: Arc<Mutex<Option<FileWatcher>>>,
    #[cfg(feature = "semantic")]
    pub semantic: Arc<Option<SemanticEngine>>,
}

impl AppState {
    pub fn new(db: Database, registry: LanguageRegistry) -> Self {
        Self {
            db: Arc::new(db),
            registry: Arc::new(registry),
            watcher: Arc::new(Mutex::new(None)),
            #[cfg(feature = "semantic")]
            semantic: Arc::new(None),
        }
    }

    #[cfg(feature = "semantic")]
    pub fn with_semantic(mut self, engine: SemanticEngine) -> Self {
        self.semantic = Arc::new(Some(engine));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_app_state_new() {
        let db = Database::init(&PathBuf::from(":memory:")).unwrap();
        let registry = LanguageRegistry::new();
        let state = AppState::new(db, registry);

        // Assert watcher is initialized to None
        let watcher_lock = state.watcher.try_lock().unwrap();
        assert!(watcher_lock.is_none());
    }

    #[cfg(feature = "semantic")]
    #[test]
    fn test_app_state_with_semantic() {
        let db = Database::init(&PathBuf::from(":memory:")).unwrap();
        let registry = LanguageRegistry::new();
        let state = AppState::new(db, registry);

        let engine = SemanticEngine::new("test_model_path".into()).unwrap(); // Assuming minimal mockable signature or real loading fails
        let state = state.with_semantic(engine);
        assert!(state.semantic.is_some());
    }
}
