//! Analysis result types for rivet-core.

use std::collections::HashMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::config::ThresholdViolation;
use crate::types::{Language, Location};

/// Halstead metrics for a single function.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HalsteadFunctionMetrics {
    /// Number of distinct operators.
    pub n1: u32,
    /// Number of distinct operands.
    pub n2: u32,
    /// Total number of operators.
    pub big_n1: u32,
    /// Total number of operands.
    pub big_n2: u32,
    /// Vocabulary: n1 + n2.
    pub vocabulary: u32,
    /// Length: N1 + N2.
    pub length: u32,
    /// Calculated length: n1*log2(n1) + n2*log2(n2).
    pub calculated_length: f64,
    /// Volume: length * log2(vocabulary).
    pub volume: f64,
    /// Difficulty: (n1/2) * (N2/n2).
    pub difficulty: f64,
    /// Effort: difficulty * volume.
    pub effort: f64,
    /// Estimated coding time in seconds: effort / 18.
    pub time: f64,
    /// Estimated bugs: volume / 3000.
    pub bugs: f64,
}

/// Halstead metrics aggregated over a file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HalsteadFileMetrics {
    /// Total volume across all functions.
    pub total_volume: f64,
    /// Total effort across all functions.
    pub total_effort: f64,
    /// Total estimated bugs.
    pub total_bugs: f64,
}

/// Analysis results for a single function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    /// Simple function name.
    pub name: String,
    /// Fully qualified name (e.g., `impl Foo::bar`).
    pub qualified_name: String,
    /// Source location.
    pub location: Location,
    /// Cyclomatic complexity (McCabe).
    pub cyclomatic: u32,
    /// Cognitive complexity (SonarSource).
    pub cognitive: u32,
    /// Maximum nesting depth.
    pub nesting_depth: u32,
    /// Number of parameters.
    pub parameter_count: u32,
    /// Non-blank, non-comment lines in function.
    pub nloc: u32,
    /// Total token count.
    pub token_count: u32,
    /// Halstead metrics.
    pub halstead: HalsteadFunctionMetrics,
}

/// Aggregate metrics for a file.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileMetrics {
    /// Non-blank lines of code.
    pub nloc: u32,
    /// Source lines of code (non-comment, non-blank).
    pub sloc: u32,
    /// Physical lines of code (total).
    pub ploc: u32,
    /// Logical lines of code (statements).
    pub lloc: u32,
    /// Comment lines.
    pub cloc: u32,
    /// Blank lines.
    pub blank: u32,
    /// Sum of cyclomatic complexity across all functions.
    pub total_cyclomatic: u32,
    /// Average cyclomatic complexity per function.
    pub avg_cyclomatic: f64,
    /// Maximum cyclomatic complexity in any function.
    pub max_cyclomatic: u32,
    /// Maintainability index (0–100).
    pub maintainability_index: f64,
    /// Halstead metrics aggregated over the file.
    pub halstead: HalsteadFileMetrics,
}

/// Analysis results for a single file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    /// Detected programming language.
    pub language: Language,
    /// Aggregate file metrics.
    pub metrics: FileMetrics,
    /// Per-function analysis results.
    pub functions: Vec<FunctionAnalysis>,
    /// Parse errors encountered (non-fatal).
    pub parse_errors: Vec<String>,
    /// Time taken to analyze in milliseconds.
    pub duration_ms: u64,
}

/// Per-language summary statistics.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageSummary {
    /// Number of files in this language.
    pub file_count: u32,
    /// Total functions.
    pub function_count: u32,
    /// Total NLOC.
    pub total_nloc: u32,
    /// Average cyclomatic complexity.
    pub avg_cyclomatic: f64,
}

/// Project-level summary.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSummary {
    /// Total number of files analyzed.
    pub total_files: u32,
    /// Total number of functions.
    pub total_functions: u32,
    /// Total non-blank lines of code.
    pub total_nloc: u32,
    /// Average cyclomatic complexity across all functions.
    pub avg_cyclomatic: f64,
    /// Average cognitive complexity across all functions.
    pub avg_cognitive: f64,
    /// Per-language summaries.
    pub language_summaries: HashMap<String, LanguageSummary>,
}

/// Analysis results for an entire project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAnalysis {
    /// Per-file analysis results.
    pub files: Vec<(PathBuf, FileAnalysis)>,
    /// Project-level summary statistics.
    pub summary: ProjectSummary,
    /// Threshold violations found.
    pub violations: Vec<ThresholdViolation>,
}
