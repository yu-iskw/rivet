//! Main analyzer entry point.

use std::path::Path;

use crate::analysis::{FileAnalysis, ProjectAnalysis, ProjectSummary};
use crate::config::{AnalyzerConfig, ThresholdResult, ThresholdViolation};
use crate::error::RivetError;
use crate::types::{FileInput, Language};

/// The main Rivet analyzer.
///
/// Construct with [`Analyzer::new`] and use [`Analyzer::analyze_source`] or
/// [`Analyzer::analyze_files`] to run analysis.
pub struct Analyzer {
    config: AnalyzerConfig,
}

impl Analyzer {
    /// Create a new analyzer with the given configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the configuration is invalid.
    pub fn new(config: AnalyzerConfig) -> Result<Self, RivetError> {
        Ok(Self { config })
    }

    /// Analyze a single source file.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails or the language is unsupported.
    pub fn analyze_source(
        &self,
        _source: &[u8],
        _language: Language,
        _file_path: Option<&Path>,
    ) -> Result<FileAnalysis, RivetError> {
        Err(RivetError::NotImplemented)
    }

    /// Analyze multiple files in parallel.
    ///
    /// # Errors
    ///
    /// Returns an error if any file cannot be analyzed.
    pub fn analyze_files(&self, files: &[FileInput]) -> Result<ProjectAnalysis, RivetError> {
        let _ = files;
        Ok(ProjectAnalysis {
            files: Vec::new(),
            summary: ProjectSummary::default(),
            violations: Vec::new(),
        })
    }

    /// Check thresholds against analysis results.
    #[must_use]
    pub fn check_thresholds(&self, analysis: &ProjectAnalysis) -> ThresholdResult {
        let violations: Vec<ThresholdViolation> = check_all_thresholds(analysis, &self.config);
        let passed = violations.is_empty();
        ThresholdResult { passed, violations }
    }

    /// Register a WASM plugin from its bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the plugin cannot be loaded.
    pub fn register_plugin(&mut self, _wasm_bytes: &[u8]) -> Result<(), RivetError> {
        Err(RivetError::NotImplemented)
    }
}

fn check_all_thresholds(
    analysis: &ProjectAnalysis,
    config: &AnalyzerConfig,
) -> Vec<ThresholdViolation> {
    let mut violations = Vec::new();
    let thresholds = &config.thresholds;

    for (path, file_analysis) in &analysis.files {
        for func in &file_analysis.functions {
            check_function_thresholds(path, func, thresholds, &mut violations);
        }
    }

    violations
}

fn check_function_thresholds(
    path: &std::path::Path,
    func: &crate::analysis::FunctionAnalysis,
    thresholds: &crate::config::Thresholds,
    violations: &mut Vec<ThresholdViolation>,
) {
    use crate::config::Severity;

    let checks: &[(&str, f64, f64)] = &[
        (
            "cyclomatic_complexity",
            f64::from(func.cyclomatic),
            f64::from(thresholds.max_cyclomatic_complexity),
        ),
        (
            "cognitive_complexity",
            f64::from(func.cognitive),
            f64::from(thresholds.max_cognitive_complexity),
        ),
        (
            "parameter_count",
            f64::from(func.parameter_count),
            f64::from(thresholds.max_parameter_count),
        ),
        (
            "function_length",
            f64::from(func.nloc),
            f64::from(thresholds.max_function_length),
        ),
        (
            "nesting_depth",
            f64::from(func.nesting_depth),
            f64::from(thresholds.max_nesting_depth),
        ),
    ];

    for (metric_name, actual, threshold) in checks {
        if *actual > *threshold {
            violations.push(ThresholdViolation {
                file_path: path.to_path_buf(),
                function_name: func.name.clone(),
                metric_name: (*metric_name).to_owned(),
                actual_value: *actual,
                threshold_value: *threshold,
                severity: Severity::Warning,
            });
        }
    }
}
