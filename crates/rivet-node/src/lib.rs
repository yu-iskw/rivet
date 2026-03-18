#![allow(
    clippy::missing_const_for_fn,
    clippy::must_use_candidate,
    clippy::needless_pass_by_value,
    clippy::unused_async,
    clippy::too_many_lines
)]

use std::{collections::HashMap, path::Path};

use napi::bindgen_prelude::{Error, Result};
use napi_derive::napi;
use rivet_core::{
    Analyzer, AnalyzerConfig, FileAnalysis, FileMetrics, FunctionAnalysis, HalsteadMetrics,
    Language, LanguageSummary, MetricValue, ParseError, PluginDiagnostic, ProjectAnalysis,
    ProjectSummary, Severity, ThresholdResult, ThresholdViolation, Thresholds,
};
use rivet_runtime::{LanguageResolution, available_languages, collect_files, resolve_language};

#[napi(object)]
pub struct AnalyzerOptions {
    pub max_cyclomatic_complexity: Option<u32>,
    pub max_cognitive_complexity: Option<u32>,
    pub max_function_length: Option<u32>,
    pub max_parameter_count: Option<u32>,
    pub max_nesting_depth: Option<u32>,
}

#[napi(object)]
pub struct JsLanguageDescriptor {
    pub id: String,
    pub display_name: String,
    pub support_level: String,
    pub source: String,
    pub extensions: Vec<String>,
}

#[napi(object)]
pub struct JsHalsteadMetrics {
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

#[napi(object)]
pub struct JsFileMetrics {
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
    pub halstead: JsHalsteadMetrics,
    pub custom_metrics: HashMap<String, serde_json::Value>,
}

#[napi(object)]
pub struct JsFunctionAnalysis {
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
    pub halstead: JsHalsteadMetrics,
    pub nesting_depth: u32,
    pub custom_metrics: HashMap<String, serde_json::Value>,
}

#[napi(object)]
pub struct JsPluginDiagnostic {
    pub plugin_name: String,
    pub function_name: Option<String>,
    pub metric_name: Option<String>,
    pub message: String,
    pub severity: String,
}

#[napi(object)]
pub struct JsParseError {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub message: String,
}

#[napi(object)]
pub struct JsFileAnalysis {
    pub file_path: Option<String>,
    pub language: String,
    pub file_metrics: JsFileMetrics,
    pub functions: Vec<JsFunctionAnalysis>,
    pub plugin_diagnostics: Vec<JsPluginDiagnostic>,
    pub parse_errors: Vec<JsParseError>,
    pub analysis_duration_ms: u32,
}

#[napi(object)]
pub struct JsLanguageSummary {
    pub files: u32,
    pub functions: u32,
    pub nloc: u32,
}

#[napi(object)]
pub struct JsProjectSummary {
    pub total_files: u32,
    pub total_functions: u32,
    pub total_nloc: u32,
    pub avg_cyclomatic: f64,
    pub avg_cognitive: f64,
    pub avg_maintainability_index: f64,
    pub languages: HashMap<String, JsLanguageSummary>,
}

#[napi(object)]
pub struct JsThresholdViolation {
    pub file_path: Option<String>,
    pub function_name: String,
    pub start_line: Option<u32>,
    pub start_column: Option<u32>,
    pub end_line: Option<u32>,
    pub end_column: Option<u32>,
    pub metric_name: String,
    pub actual_value: f64,
    pub threshold_value: f64,
    pub severity: String,
}

#[napi(object)]
pub struct JsThresholdResult {
    pub passed: bool,
    pub violations: Vec<JsThresholdViolation>,
}

#[napi(object)]
pub struct JsProjectAnalysis {
    pub files: Vec<JsFileAnalysis>,
    pub summary: JsProjectSummary,
    pub threshold_violations: Vec<JsThresholdViolation>,
}

impl AnalyzerOptions {
    fn into_config(self) -> AnalyzerConfig {
        AnalyzerConfig {
            thresholds: Thresholds {
                max_cyclomatic_complexity: self.max_cyclomatic_complexity,
                max_cognitive_complexity: self.max_cognitive_complexity,
                max_function_length: self.max_function_length,
                max_parameter_count: self.max_parameter_count,
                max_nesting_depth: self.max_nesting_depth,
                ..Thresholds::default()
            },
            ..AnalyzerConfig::default()
        }
    }
}

impl From<rivet_runtime::LanguageDescriptor> for JsLanguageDescriptor {
    fn from(inner: rivet_runtime::LanguageDescriptor) -> Self {
        Self {
            id: inner.id,
            display_name: inner.display_name,
            support_level: inner.support_level.as_str().to_string(),
            source: inner.source.as_str().to_string(),
            extensions: inner.extensions,
        }
    }
}

#[napi]
pub struct JsAnalyzer {
    inner: Analyzer,
}

#[napi]
impl JsAnalyzer {
    #[napi(constructor)]
    pub fn new(options: Option<AnalyzerOptions>) -> Result<Self> {
        let config = options.map_or_else(AnalyzerConfig::default, AnalyzerOptions::into_config);
        Analyzer::new(config)
            .map(|inner| Self { inner })
            .map_err(to_napi_err)
    }

    #[napi]
    pub fn analyze_source(
        &self,
        source: String,
        language: String,
        file_path: Option<String>,
    ) -> Result<JsFileAnalysis> {
        let language = parse_language(&language).map_err(to_napi_err)?;
        self.inner
            .analyze_source(
                source.as_bytes(),
                language,
                file_path.as_deref().map(Path::new),
            )
            .map(Into::into)
            .map_err(to_napi_err)
    }

    #[napi]
    pub async fn analyze_directory(
        &self,
        path: String,
        language: Option<String>,
    ) -> Result<JsProjectAnalysis> {
        self.analyze_directory_internal(path, language)
            .map(Into::into)
    }

    #[napi]
    pub fn check_thresholds(&self, analysis: JsProjectAnalysis) -> JsThresholdResult {
        self.inner
            .check_thresholds(&project_for_threshold_check(analysis))
            .into()
    }

    #[napi]
    pub fn supported_languages(&self) -> Vec<String> {
        self.inner
            .supported_languages()
            .into_iter()
            .map(|language| language.as_str().to_string())
            .collect()
    }

    #[napi]
    pub fn available_languages(&self) -> Vec<JsLanguageDescriptor> {
        available_languages().into_iter().map(Into::into).collect()
    }
}

fn parse_language(language: &str) -> Result<Language> {
    match resolve_language(language).map_err(to_napi_err)? {
        LanguageResolution::Full { language, .. } => Ok(language),
        LanguageResolution::ParseOnly(descriptor) => Err(Error::from_reason(format!(
            "recognized but parse-only language `{}`",
            descriptor.id
        ))),
    }
}

impl JsAnalyzer {
    fn analyze_directory_internal(
        &self,
        path: String,
        language: Option<String>,
    ) -> Result<ProjectAnalysis> {
        let collected =
            collect_files(Path::new(&path), language.as_deref(), None).map_err(to_napi_err)?;
        self.inner
            .analyze_files(&collected.analyzable)
            .map_err(to_napi_err)
    }
}

impl From<HalsteadMetrics> for JsHalsteadMetrics {
    fn from(metrics: HalsteadMetrics) -> Self {
        Self {
            n1: metrics.n1,
            n2: metrics.n2,
            big_n1: metrics.big_n1,
            big_n2: metrics.big_n2,
            vocabulary: metrics.vocabulary,
            length: metrics.length,
            calculated_length: metrics.calculated_length,
            volume: metrics.volume,
            difficulty: metrics.difficulty,
            effort: metrics.effort,
            time: metrics.time,
            bugs: metrics.bugs,
        }
    }
}

impl From<FileMetrics> for JsFileMetrics {
    fn from(metrics: FileMetrics) -> Self {
        Self {
            nloc: metrics.nloc,
            sloc: metrics.sloc,
            ploc: metrics.ploc,
            lloc: metrics.lloc,
            cloc: metrics.cloc,
            blank: metrics.blank,
            total_complexity: metrics.total_complexity,
            avg_complexity: metrics.avg_complexity,
            max_complexity: metrics.max_complexity,
            maintainability_index: metrics.maintainability_index,
            halstead: metrics.halstead.into(),
            custom_metrics: metric_map_to_json(metrics.custom_metrics),
        }
    }
}

impl From<FunctionAnalysis> for JsFunctionAnalysis {
    fn from(function: FunctionAnalysis) -> Self {
        Self {
            name: function.name,
            qualified_name: function.qualified_name,
            start_line: function.start_line,
            end_line: function.end_line,
            start_column: function.start_column,
            end_column: function.end_column,
            cyclomatic_complexity: function.cyclomatic_complexity,
            cognitive_complexity: function.cognitive_complexity,
            parameter_count: function.parameter_count,
            token_count: function.token_count,
            nloc: function.nloc,
            halstead: function.halstead.into(),
            nesting_depth: function.nesting_depth,
            custom_metrics: metric_map_to_json(function.custom_metrics),
        }
    }
}

impl From<PluginDiagnostic> for JsPluginDiagnostic {
    fn from(diagnostic: PluginDiagnostic) -> Self {
        Self {
            plugin_name: diagnostic.plugin_name,
            function_name: diagnostic.function_name,
            metric_name: diagnostic.metric_name,
            message: diagnostic.message,
            severity: severity_label(diagnostic.severity).to_string(),
        }
    }
}

impl From<ParseError> for JsParseError {
    fn from(error: ParseError) -> Self {
        Self {
            start_line: error.start_line,
            start_column: error.start_column,
            end_line: error.end_line,
            end_column: error.end_column,
            message: error.message,
        }
    }
}

impl From<FileAnalysis> for JsFileAnalysis {
    fn from(analysis: FileAnalysis) -> Self {
        Self {
            file_path: analysis.file_path.map(|path| path.display().to_string()),
            language: analysis.language.as_str().to_string(),
            file_metrics: analysis.file_metrics.into(),
            functions: analysis.functions.into_iter().map(Into::into).collect(),
            plugin_diagnostics: analysis
                .plugin_diagnostics
                .into_iter()
                .map(Into::into)
                .collect(),
            parse_errors: analysis.parse_errors.into_iter().map(Into::into).collect(),
            analysis_duration_ms: u32::try_from(analysis.analysis_duration.as_millis())
                .unwrap_or(u32::MAX),
        }
    }
}

impl From<LanguageSummary> for JsLanguageSummary {
    fn from(summary: LanguageSummary) -> Self {
        Self {
            files: summary.files,
            functions: summary.functions,
            nloc: summary.nloc,
        }
    }
}

impl From<ProjectSummary> for JsProjectSummary {
    fn from(summary: ProjectSummary) -> Self {
        Self {
            total_files: summary.total_files,
            total_functions: summary.total_functions,
            total_nloc: summary.total_nloc,
            avg_cyclomatic: summary.avg_cyclomatic,
            avg_cognitive: summary.avg_cognitive,
            avg_maintainability_index: summary.avg_maintainability_index,
            languages: summary
                .languages
                .into_iter()
                .map(|(language, summary)| (language.as_str().to_string(), summary.into()))
                .collect(),
        }
    }
}

impl From<ThresholdViolation> for JsThresholdViolation {
    fn from(violation: ThresholdViolation) -> Self {
        Self {
            file_path: violation.file_path.map(|path| path.display().to_string()),
            function_name: violation.function_name,
            start_line: violation.start_line,
            start_column: violation.start_column,
            end_line: violation.end_line,
            end_column: violation.end_column,
            metric_name: violation.metric_name,
            actual_value: violation.actual_value,
            threshold_value: violation.threshold_value,
            severity: severity_label(violation.severity).to_string(),
        }
    }
}

impl From<ThresholdResult> for JsThresholdResult {
    fn from(result: ThresholdResult) -> Self {
        Self {
            passed: result.passed,
            violations: result.violations.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ProjectAnalysis> for JsProjectAnalysis {
    fn from(analysis: ProjectAnalysis) -> Self {
        Self {
            files: analysis.files.into_iter().map(Into::into).collect(),
            summary: analysis.summary.into(),
            threshold_violations: analysis
                .threshold_violations
                .into_iter()
                .map(Into::into)
                .collect(),
        }
    }
}

fn metric_map_to_json(metrics: HashMap<String, MetricValue>) -> HashMap<String, serde_json::Value> {
    metrics
        .into_iter()
        .map(|(name, value)| (name, metric_value_to_json(value)))
        .collect()
}

fn metric_value_to_json(value: MetricValue) -> serde_json::Value {
    match value {
        MetricValue::Integer(value) => serde_json::Value::from(value),
        MetricValue::Float(value) => serde_json::Value::from(value),
        MetricValue::Composite(values) => serde_json::Value::Object(
            values
                .into_iter()
                .map(|(name, value)| (name, metric_value_to_json(value)))
                .collect(),
        ),
    }
}

fn project_for_threshold_check(analysis: JsProjectAnalysis) -> ProjectAnalysis {
    ProjectAnalysis {
        files: Vec::new(),
        summary: ProjectSummary::default(),
        threshold_violations: analysis
            .threshold_violations
            .into_iter()
            .map(|violation| ThresholdViolation {
                file_path: violation.file_path.map(Into::into),
                function_name: violation.function_name,
                start_line: violation.start_line,
                start_column: violation.start_column,
                end_line: violation.end_line,
                end_column: violation.end_column,
                metric_name: violation.metric_name,
                actual_value: violation.actual_value,
                threshold_value: violation.threshold_value,
                severity: if violation.severity.eq_ignore_ascii_case("error") {
                    Severity::Error
                } else {
                    Severity::Warning
                },
            })
            .collect(),
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

fn to_napi_err(error: impl ToString) -> Error {
    Error::from_reason(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn node_binding_returns_typed_file_analysis() {
        let analyzer = JsAnalyzer::new(None).expect("analyzer");
        let analysis = analyzer
            .analyze_source(
                "fn sample(value: i32) -> i32 { if value > 0 { value } else { 0 } }".to_string(),
                "rust".to_string(),
                None,
            )
            .expect("analysis should succeed");

        assert_eq!(analysis.language, "rust");
        assert_eq!(analysis.functions[0].name, "sample");
        assert!(
            analyzer
                .supported_languages()
                .iter()
                .any(|item| item == "rust")
        );
    }

    #[test]
    fn available_languages_expose_metadata() {
        let analyzer = JsAnalyzer::new(None).expect("analyzer");
        let languages = analyzer.available_languages();

        assert!(languages.len() > analyzer.supported_languages().len());
        assert!(languages.iter().any(|language| language.id == "rust"));
        assert!(languages.iter().any(|language| language.id == "swift"));
        assert!(
            languages
                .iter()
                .any(|language| language.support_level == "parse_only")
        );
    }

    #[test]
    fn parse_only_source_is_rejected_explicitly() {
        let analyzer = JsAnalyzer::new(None).expect("analyzer");
        let result = analyzer.analyze_source(
            "func sample(value: Int) -> Int { value }".to_string(),
            "swift".to_string(),
            None,
        );

        match result {
            Ok(_) => panic!("parse-only language should fail"),
            Err(error) => assert!(error.to_string().contains("parse-only")),
        }
    }

    #[test]
    fn node_binding_threshold_checks_accept_typed_project_analysis() {
        let analyzer = JsAnalyzer::new(Some(AnalyzerOptions {
            max_cyclomatic_complexity: Some(1),
            max_cognitive_complexity: Some(1),
            max_function_length: Some(1),
            max_parameter_count: Some(1),
            max_nesting_depth: Some(1),
        }))
        .expect("analyzer");
        let fixture_root =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/rust");

        let project = analyzer
            .analyze_directory_internal(
                fixture_root.display().to_string(),
                Some("rust".to_string()),
            )
            .expect("project analysis");
        let threshold_result = analyzer.check_thresholds(project.into());

        assert!(!threshold_result.passed);
        assert!(!threshold_result.violations.is_empty());
    }
}
