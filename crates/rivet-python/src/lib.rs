use std::str::FromStr;

use rivet_core::{Analyzer, AnalyzerConfig, FileAnalysis, Language, RivetError};

pub struct PyAnalyzer {
    inner: Analyzer,
}

impl PyAnalyzer {
    pub fn new(config: Option<AnalyzerConfig>) -> Result<Self, RivetError> {
        Ok(Self {
            inner: Analyzer::new(config.unwrap_or_default())?,
        })
    }

    pub fn analyze_source(&self, source: &str, language: &str) -> Result<FileAnalysis, RivetError> {
        self.inner
            .analyze_source(source.as_bytes(), Language::from_str(language)?, None)
    }
}
