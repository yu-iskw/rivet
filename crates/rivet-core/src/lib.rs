pub mod analyzer;
pub mod config;
pub mod error;
pub mod language;
pub mod metrics;
pub mod output;
pub mod parser;
pub mod plugin;
pub mod types;

pub use analyzer::Analyzer;
pub use config::{AnalyzerConfig, PathThresholdOverride, PluginConfig, PluginEntryConfig};
pub use error::RivetError;
pub use language::{
    Language, LanguageConfig, LanguageDescriptor, LanguageRegistry, LanguageSource,
    LanguageSupportLevel, analysis_fingerprint,
};
pub use types::{
    FileAnalysis, FileInput, FileMetrics, FunctionAnalysis, HalsteadMetrics, LanguageSummary,
    MetricValue, ParseError, PluginDiagnostic, ProjectAnalysis, ProjectSummary, Severity,
    ThresholdResult, ThresholdViolation, Thresholds,
};
