use std::{collections::HashMap, path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

use crate::language::Language;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileInput {
    pub file_path: Option<PathBuf>,
    pub language: Language,
    pub source: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub file_path: Option<PathBuf>,
    pub language: Language,
    pub file_metrics: FileMetrics,
    pub functions: Vec<FunctionAnalysis>,
    pub parse_errors: Vec<ParseError>,
    pub analysis_duration: Duration,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileMetrics {
    pub nloc: u32,
    pub sloc: u32,
    pub ploc: u32,
    pub lloc: u32,
    pub cloc: u32,
    pub blank: u32,
    pub total_complexity: f64,
    pub avg_complexity: f64,
    pub max_complexity: f64,
    pub maintainability_index: f64,
    pub halstead: HalsteadMetrics,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    pub name: String,
    pub qualified_name: String,
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub cyclomatic_complexity: u32,
    pub cognitive_complexity: u32,
    pub parameter_count: u32,
    pub token_count: u32,
    pub nloc: u32,
    pub halstead: HalsteadMetrics,
    pub nesting_depth: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HalsteadMetrics {
    pub n1: u32,
    pub n2: u32,
    pub big_n1: u32,
    pub big_n2: u32,
    pub vocabulary: u32,
    pub length: u32,
    pub calculated_length: f64,
    pub volume: f64,
    pub difficulty: f64,
    pub effort: f64,
    pub time: f64,
    pub bugs: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectAnalysis {
    pub files: Vec<FileAnalysis>,
    pub summary: ProjectSummary,
    pub threshold_violations: Vec<ThresholdViolation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectSummary {
    pub total_files: u32,
    pub total_functions: u32,
    pub total_nloc: u32,
    pub avg_cyclomatic: f64,
    pub avg_cognitive: f64,
    pub avg_maintainability_index: f64,
    pub languages: HashMap<Language, LanguageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LanguageSummary {
    pub files: u32,
    pub functions: u32,
    pub nloc: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    pub max_cyclomatic_complexity: Option<u32>,
    pub max_cognitive_complexity: Option<u32>,
    pub max_function_length: Option<u32>,
    pub max_parameter_count: Option<u32>,
    pub max_nesting_depth: Option<u32>,
    pub min_maintainability_index: Option<f64>,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            max_cyclomatic_complexity: Some(15),
            max_cognitive_complexity: Some(15),
            max_function_length: Some(100),
            max_parameter_count: Some(5),
            max_nesting_depth: Some(5),
            min_maintainability_index: Some(20.0),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    pub file_path: Option<PathBuf>,
    pub function_name: String,
    pub metric_name: String,
    pub actual_value: f64,
    pub threshold_value: f64,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Severity {
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ThresholdResult {
    pub passed: bool,
    pub violations: Vec<ThresholdViolation>,
}
