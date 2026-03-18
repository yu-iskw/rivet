use thiserror::Error;

#[derive(Debug, Error)]
pub enum RivetError {
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    #[error("failed to initialize language {language}: {message}")]
    QueryCompilation { language: String, message: String },
    #[error("missing query capture {capture} for language {language}")]
    MissingQueryCapture { language: String, capture: String },
    #[error("parse failure: {0}")]
    Parse(String),
    #[error("analysis failure: {0}")]
    Analysis(String),
    #[error("plugin failure: {0}")]
    Plugin(String),
    #[error("serialization failure: {0}")]
    Serialization(String),
    #[error("unsupported feature: {0}")]
    UnsupportedFeature(String),
}
