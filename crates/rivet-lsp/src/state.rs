use std::sync::Arc;

use dashmap::DashMap;
use rivet_core::{FileAnalysis, Language};

#[derive(Debug, Clone)]
pub struct DocumentEntry {
    pub source: String,
    pub language: Language,
    pub version: i32,
    pub revision: u64,
    pub dirty: bool,
    pub analysis: Option<FileAnalysis>,
}

#[derive(Clone, Default)]
pub struct DocumentState {
    documents: Arc<DashMap<String, DocumentEntry>>,
}

impl DocumentState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&self, uri: &str, source: String, version: i32, language: Language) {
        self.documents.insert(
            uri.to_string(),
            DocumentEntry {
                source,
                language,
                version,
                revision: 1,
                dirty: false,
                analysis: None,
            },
        );
    }

    #[must_use]
    pub fn update_source(&self, uri: &str, source: String, version: i32) -> Option<DocumentEntry> {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.source = source;
            entry.version = version;
            entry.revision = entry.revision.saturating_add(1);
            entry.dirty = true;
            entry.analysis = None;
            Some(entry.clone())
        } else {
            None
        }
    }

    #[must_use]
    pub fn replace_saved_text(&self, uri: &str, source: String) -> Option<DocumentEntry> {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.source = source;
            entry.revision = entry.revision.saturating_add(1);
            entry.dirty = false;
            entry.analysis = None;
            Some(entry.clone())
        } else {
            None
        }
    }

    #[must_use]
    pub fn bump_revision(&self, uri: &str) -> Option<DocumentEntry> {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.revision = entry.revision.saturating_add(1);
            entry.analysis = None;
            Some(entry.clone())
        } else {
            None
        }
    }

    #[must_use]
    pub fn set_analysis_if_revision(
        &self,
        uri: &str,
        revision: u64,
        analysis: Option<FileAnalysis>,
    ) -> Option<DocumentEntry> {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            if entry.revision != revision {
                return None;
            }
            entry.analysis = analysis;
            Some(entry.clone())
        } else {
            None
        }
    }

    #[must_use]
    pub fn get(&self, uri: &str) -> Option<DocumentEntry> {
        self.documents.get(uri).map(|entry| entry.clone())
    }

    #[must_use]
    pub fn remove(&self, uri: &str) -> Option<DocumentEntry> {
        self.documents.remove(uri).map(|(_, entry)| entry)
    }

    #[must_use]
    pub fn uris(&self) -> Vec<String> {
        self.documents
            .iter()
            .map(|entry| entry.key().clone())
            .collect()
    }
}
