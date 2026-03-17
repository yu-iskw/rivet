//! Error types for rivet-core.

/// Errors that can occur during analysis.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum RivetError {
    /// A parse error occurred.
    #[error("parse error: {0}")]
    Parse(String),
    /// The language is not supported.
    #[error("unsupported language: {0}")]
    UnsupportedLanguage(String),
    /// An IO error occurred (only in consumers, not core).
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A plugin error occurred.
    #[error("plugin error: {0}")]
    Plugin(String),
    /// Feature not yet implemented.
    #[error("not implemented")]
    NotImplemented,
}
