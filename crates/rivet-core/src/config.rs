//! Configuration types for rivet-core.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::Language;

/// Threshold configuration for code complexity analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    /// Maximum cyclomatic complexity per function.
    pub max_cyclomatic_complexity: u32,
    /// Maximum cognitive complexity per function.
    pub max_cognitive_complexity: u32,
    /// Maximum function length in lines.
    pub max_function_length: u32,
    /// Maximum number of parameters.
    pub max_parameter_count: u32,
    /// Maximum nesting depth.
    pub max_nesting_depth: u32,
    /// Minimum maintainability index (0–100).
    pub min_maintainability_index: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            max_cyclomatic_complexity: 15,
            max_cognitive_complexity: 15,
            max_function_length: 100,
            max_parameter_count: 5,
            max_nesting_depth: 5,
            min_maintainability_index: 20.0,
        }
    }
}

/// Configuration for the Analyzer.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyzerConfig {
    /// Languages to analyze (empty = auto-detect from file extension).
    pub languages: Vec<Language>,
    /// Threshold configuration.
    pub thresholds: Thresholds,
    /// Number of parallel jobs (0 = use num_cpus).
    pub jobs: usize,
    /// Paths to WASM plugin files.
    pub plugin_paths: Vec<PathBuf>,
}

/// Severity of a threshold violation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Warning — exceeds threshold but may be acceptable.
    Warning,
    /// Error — clearly exceeds threshold.
    Error,
}

/// A single threshold violation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    /// File path where the violation occurred.
    pub file_path: std::path::PathBuf,
    /// Function name where the violation occurred.
    pub function_name: String,
    /// The metric that was violated.
    pub metric_name: String,
    /// The actual value of the metric.
    pub actual_value: f64,
    /// The threshold value.
    pub threshold_value: f64,
    /// Severity of the violation.
    pub severity: Severity,
}

/// Result of checking thresholds against an analysis.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThresholdResult {
    /// Whether all thresholds passed.
    pub passed: bool,
    /// List of violations.
    pub violations: Vec<ThresholdViolation>,
}
