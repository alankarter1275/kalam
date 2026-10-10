pub mod traits;

pub use traits::*;

use std::sync::{Arc, OnceLock};

pub static GLOBAL_SOURCE_MANAGER: OnceLock<Arc<SourceManager>> = OnceLock::new();

pub fn global_source_manager() -> Arc<SourceManager> {
    GLOBAL_SOURCE_MANAGER
        .get_or_init(|| Arc::new(SourceManager::new()))
        .clone()
}

pub struct SourceManager {
    sources: Vec<Arc<dyn Source>>,
}

impl SourceManager {
    pub fn new() -> Self {
        Self {
            sources: Vec::new(),
        }
    }

    #[allow(dead_code)]
    // This will be used in Phase 4 when registering dynamic Wasm plugins.
    pub fn register(&mut self, source: Arc<dyn Source>) {
        self.sources.push(source);
    }

    pub fn get(&self, id: &str) -> Option<Arc<dyn Source>> {
        self.sources.iter().find(|s| s.id() == id).cloned()
    }

    pub fn all(&self) -> Vec<Arc<dyn Source>> {
        self.sources.clone()
    }
}

impl Default for SourceManager {
    fn default() -> Self {
        Self::new()
    }
}
