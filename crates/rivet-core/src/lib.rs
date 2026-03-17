//! Rivet core library for code complexity analysis.
//!
//! This is a pure Rust library with no IO, async, or CLI dependencies.
//! Entry points take `&[u8]` source and return structured results.

pub mod analysis;
pub mod analyzer;
pub mod config;
pub mod error;
pub mod languages;
pub mod metrics;
pub mod output;
pub mod parser;
pub mod plugin;
pub mod types;

pub use analysis::{
    FileAnalysis, FileMetrics, FunctionAnalysis, HalsteadFileMetrics, HalsteadFunctionMetrics,
    LanguageSummary, ProjectAnalysis, ProjectSummary,
};
pub use analyzer::Analyzer;
pub use config::{AnalyzerConfig, Severity, ThresholdResult, ThresholdViolation, Thresholds};
pub use error::RivetError;
pub use types::{FileInput, Language, Location};
