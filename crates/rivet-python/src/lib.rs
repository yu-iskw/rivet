#![allow(
    clippy::missing_const_for_fn,
    clippy::needless_pass_by_value,
    clippy::too_many_lines
)]

use std::{collections::HashMap, path::Path};

use pyo3::{IntoPyObjectExt, prelude::*, types::PyDict};
use rivet_core::{
    Analyzer, AnalyzerConfig, FileAnalysis, FileMetrics, FunctionAnalysis, HalsteadMetrics,
    Language, LanguageSummary, MetricValue, ParseError, PluginDiagnostic, ProjectAnalysis,
    ProjectSummary, Severity, ThresholdResult, ThresholdViolation, Thresholds,
};
use rivet_runtime::{LanguageResolution, available_languages, collect_files, resolve_language};

#[pyclass(name = "Analyzer")]
pub struct PyAnalyzer {
    inner: Analyzer,
}

#[pyclass(name = "LanguageDescriptor", skip_from_py_object)]
#[derive(Clone)]
pub struct PyLanguageDescriptor {
    inner: rivet_runtime::LanguageDescriptor,
}

#[pyclass(name = "HalsteadMetrics", skip_from_py_object)]
#[derive(Clone)]
pub struct PyHalsteadMetrics {
    inner: HalsteadMetrics,
}

#[pyclass(name = "FileMetrics", skip_from_py_object)]
#[derive(Clone)]
pub struct PyFileMetrics {
    inner: FileMetrics,
}

#[pyclass(name = "FunctionAnalysis", skip_from_py_object)]
#[derive(Clone)]
pub struct PyFunctionAnalysis {
    inner: FunctionAnalysis,
}

#[pyclass(name = "PluginDiagnostic", skip_from_py_object)]
#[derive(Clone)]
pub struct PyPluginDiagnostic {
    inner: PluginDiagnostic,
}

#[pyclass(name = "ParseError", skip_from_py_object)]
#[derive(Clone)]
pub struct PyParseError {
    inner: ParseError,
}

#[pyclass(name = "FileAnalysis", skip_from_py_object)]
#[derive(Clone)]
pub struct PyFileAnalysis {
    inner: FileAnalysis,
}

#[pyclass(name = "LanguageSummary", skip_from_py_object)]
#[derive(Clone)]
pub struct PyLanguageSummary {
    inner: LanguageSummary,
}

#[pyclass(name = "ProjectSummary", skip_from_py_object)]
#[derive(Clone)]
pub struct PyProjectSummary {
    inner: ProjectSummary,
}

#[pyclass(name = "ThresholdViolation", skip_from_py_object)]
#[derive(Clone)]
pub struct PyThresholdViolation {
    inner: ThresholdViolation,
}

#[pyclass(name = "ThresholdResult", skip_from_py_object)]
#[derive(Clone)]
pub struct PyThresholdResult {
    inner: ThresholdResult,
}

#[pyclass(name = "ProjectAnalysis", skip_from_py_object)]
#[derive(Clone)]
pub struct PyProjectAnalysis {
    inner: ProjectAnalysis,
}

#[pymethods]
impl PyAnalyzer {
    #[new]
    #[pyo3(signature = (
        max_cyclomatic_complexity=None,
        max_cognitive_complexity=None,
        max_function_length=None,
        max_parameter_count=None,
        max_nesting_depth=None
    ))]
    fn new(
        max_cyclomatic_complexity: Option<u32>,
        max_cognitive_complexity: Option<u32>,
        max_function_length: Option<u32>,
        max_parameter_count: Option<u32>,
        max_nesting_depth: Option<u32>,
    ) -> PyResult<Self> {
        let config = AnalyzerConfig {
            thresholds: Thresholds {
                max_cyclomatic_complexity,
                max_cognitive_complexity,
                max_function_length,
                max_parameter_count,
                max_nesting_depth,
                ..Thresholds::default()
            },
            ..AnalyzerConfig::default()
        };
        Ok(Self {
            inner: Analyzer::new(config).map_err(to_py_err)?,
        })
    }

    #[pyo3(signature = (source, language, file_path=None))]
    fn analyze_source(
        &self,
        source: &str,
        language: &str,
        file_path: Option<&str>,
    ) -> PyResult<PyFileAnalysis> {
        let language = parse_language(language)?;
        let analysis = self
            .inner
            .analyze_source(source.as_bytes(), language, file_path.map(Path::new))
            .map_err(to_py_err)?;
        Ok(analysis.into())
    }

    #[pyo3(signature = (path, language=None))]
    fn analyze_directory(&self, path: &str, language: Option<&str>) -> PyResult<PyProjectAnalysis> {
        let collected = collect_files(Path::new(path), language, None).map_err(to_py_err)?;
        let analysis = self
            .inner
            .analyze_files(&collected.analyzable)
            .map_err(to_py_err)?;
        Ok(analysis.into())
    }

    fn check_thresholds(&self, analysis: PyRef<'_, PyProjectAnalysis>) -> PyThresholdResult {
        self.inner.check_thresholds(&analysis.inner).into()
    }

    fn supported_languages(&self) -> Vec<String> {
        self.inner
            .supported_languages()
            .into_iter()
            .map(|language| language.as_str().to_string())
            .collect()
    }

    #[staticmethod]
    fn available_languages() -> Vec<PyLanguageDescriptor> {
        available_languages().into_iter().map(Into::into).collect()
    }
}

#[pymethods]
impl PyLanguageDescriptor {
    #[getter]
    fn id(&self) -> String {
        self.inner.id.clone()
    }

    #[getter]
    fn display_name(&self) -> String {
        self.inner.display_name.clone()
    }

    #[getter]
    fn support_level(&self) -> String {
        self.inner.support_level.as_str().to_string()
    }

    #[getter]
    fn source(&self) -> String {
        self.inner.source.as_str().to_string()
    }

    #[getter]
    fn extensions(&self) -> Vec<String> {
        self.inner.extensions.clone()
    }
}

impl From<rivet_runtime::LanguageDescriptor> for PyLanguageDescriptor {
    fn from(inner: rivet_runtime::LanguageDescriptor) -> Self {
        Self { inner }
    }
}

#[pymethods]
impl PyHalsteadMetrics {
    #[getter]
    fn n1(&self) -> u32 {
        self.inner.n1
    }

    #[getter]
    fn n2(&self) -> u32 {
        self.inner.n2
    }

    #[getter]
    fn big_n1(&self) -> u32 {
        self.inner.big_n1
    }

    #[getter]
    fn big_n2(&self) -> u32 {
        self.inner.big_n2
    }

    #[getter]
    fn vocabulary(&self) -> u32 {
        self.inner.vocabulary
    }

    #[getter]
    fn length(&self) -> u32 {
        self.inner.length
    }

    #[getter]
    fn calculated_length(&self) -> f64 {
        self.inner.calculated_length
    }

    #[getter]
    fn volume(&self) -> f64 {
        self.inner.volume
    }

    #[getter]
    fn difficulty(&self) -> f64 {
        self.inner.difficulty
    }

    #[getter]
    fn effort(&self) -> f64 {
        self.inner.effort
    }

    #[getter]
    fn time(&self) -> f64 {
        self.inner.time
    }

    #[getter]
    fn bugs(&self) -> f64 {
        self.inner.bugs
    }
}

#[pymethods]
impl PyFileMetrics {
    #[getter]
    fn nloc(&self) -> u32 {
        self.inner.nloc
    }

    #[getter]
    fn sloc(&self) -> u32 {
        self.inner.sloc
    }

    #[getter]
    fn ploc(&self) -> u32 {
        self.inner.ploc
    }

    #[getter]
    fn lloc(&self) -> u32 {
        self.inner.lloc
    }

    #[getter]
    fn cloc(&self) -> u32 {
        self.inner.cloc
    }

    #[getter]
    fn blank(&self) -> u32 {
        self.inner.blank
    }

    #[getter]
    fn total_complexity(&self) -> f64 {
        self.inner.total_complexity
    }

    #[getter]
    fn avg_complexity(&self) -> f64 {
        self.inner.avg_complexity
    }

    #[getter]
    fn max_complexity(&self) -> f64 {
        self.inner.max_complexity
    }

    #[getter]
    fn maintainability_index(&self) -> f64 {
        self.inner.maintainability_index
    }

    #[getter]
    fn halstead(&self) -> PyHalsteadMetrics {
        self.inner.halstead.clone().into()
    }

    #[getter]
    fn custom_metrics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        metric_map_to_py(py, &self.inner.custom_metrics)
    }
}

#[pymethods]
impl PyFunctionAnalysis {
    #[getter]
    fn name(&self) -> String {
        self.inner.name.clone()
    }

    #[getter]
    fn qualified_name(&self) -> String {
        self.inner.qualified_name.clone()
    }

    #[getter]
    fn start_line(&self) -> u32 {
        self.inner.start_line
    }

    #[getter]
    fn end_line(&self) -> u32 {
        self.inner.end_line
    }

    #[getter]
    fn start_column(&self) -> u32 {
        self.inner.start_column
    }

    #[getter]
    fn end_column(&self) -> u32 {
        self.inner.end_column
    }

    #[getter]
    fn cyclomatic_complexity(&self) -> u32 {
        self.inner.cyclomatic_complexity
    }

    #[getter]
    fn cognitive_complexity(&self) -> u32 {
        self.inner.cognitive_complexity
    }

    #[getter]
    fn parameter_count(&self) -> u32 {
        self.inner.parameter_count
    }

    #[getter]
    fn token_count(&self) -> u32 {
        self.inner.token_count
    }

    #[getter]
    fn nloc(&self) -> u32 {
        self.inner.nloc
    }

    #[getter]
    fn halstead(&self) -> PyHalsteadMetrics {
        self.inner.halstead.clone().into()
    }

    #[getter]
    fn nesting_depth(&self) -> u32 {
        self.inner.nesting_depth
    }

    #[getter]
    fn custom_metrics(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        metric_map_to_py(py, &self.inner.custom_metrics)
    }
}

#[pymethods]
impl PyPluginDiagnostic {
    #[getter]
    fn plugin_name(&self) -> String {
        self.inner.plugin_name.clone()
    }

    #[getter]
    fn function_name(&self) -> Option<String> {
        self.inner.function_name.clone()
    }

    #[getter]
    fn metric_name(&self) -> Option<String> {
        self.inner.metric_name.clone()
    }

    #[getter]
    fn message(&self) -> String {
        self.inner.message.clone()
    }

    #[getter]
    fn severity(&self) -> String {
        severity_label(self.inner.severity).to_string()
    }
}

#[pymethods]
impl PyParseError {
    #[getter]
    fn start_line(&self) -> u32 {
        self.inner.start_line
    }

    #[getter]
    fn start_column(&self) -> u32 {
        self.inner.start_column
    }

    #[getter]
    fn end_line(&self) -> u32 {
        self.inner.end_line
    }

    #[getter]
    fn end_column(&self) -> u32 {
        self.inner.end_column
    }

    #[getter]
    fn message(&self) -> String {
        self.inner.message.clone()
    }
}

#[pymethods]
impl PyFileAnalysis {
    #[getter]
    fn file_path(&self) -> Option<String> {
        self.inner
            .file_path
            .as_ref()
            .map(|path| path.display().to_string())
    }

    #[getter]
    fn language(&self) -> String {
        self.inner.language.as_str().to_string()
    }

    #[getter]
    fn file_metrics(&self) -> PyFileMetrics {
        self.inner.file_metrics.clone().into()
    }

    #[getter]
    fn functions(&self) -> Vec<PyFunctionAnalysis> {
        self.inner
            .functions
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    #[getter]
    fn plugin_diagnostics(&self) -> Vec<PyPluginDiagnostic> {
        self.inner
            .plugin_diagnostics
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    #[getter]
    fn parse_errors(&self) -> Vec<PyParseError> {
        self.inner
            .parse_errors
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }

    #[getter]
    fn analysis_duration_ms(&self) -> u64 {
        u64::try_from(self.inner.analysis_duration.as_millis()).unwrap_or(u64::MAX)
    }
}

#[pymethods]
impl PyLanguageSummary {
    #[getter]
    fn files(&self) -> u32 {
        self.inner.files
    }

    #[getter]
    fn functions(&self) -> u32 {
        self.inner.functions
    }

    #[getter]
    fn nloc(&self) -> u32 {
        self.inner.nloc
    }
}

#[pymethods]
impl PyProjectSummary {
    #[getter]
    fn total_files(&self) -> u32 {
        self.inner.total_files
    }

    #[getter]
    fn total_functions(&self) -> u32 {
        self.inner.total_functions
    }

    #[getter]
    fn total_nloc(&self) -> u32 {
        self.inner.total_nloc
    }

    #[getter]
    fn avg_cyclomatic(&self) -> f64 {
        self.inner.avg_cyclomatic
    }

    #[getter]
    fn avg_cognitive(&self) -> f64 {
        self.inner.avg_cognitive
    }

    #[getter]
    fn avg_maintainability_index(&self) -> f64 {
        self.inner.avg_maintainability_index
    }

    #[getter]
    fn languages(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let dict = PyDict::new(py);
        for (language, summary) in &self.inner.languages {
            dict.set_item(language.as_str(), PyLanguageSummary::from(summary.clone()))?;
        }
        dict.into_py_any(py)
    }
}

#[pymethods]
impl PyThresholdViolation {
    #[getter]
    fn file_path(&self) -> Option<String> {
        self.inner
            .file_path
            .as_ref()
            .map(|path| path.display().to_string())
    }

    #[getter]
    fn function_name(&self) -> String {
        self.inner.function_name.clone()
    }

    #[getter]
    fn start_line(&self) -> Option<u32> {
        self.inner.start_line
    }

    #[getter]
    fn start_column(&self) -> Option<u32> {
        self.inner.start_column
    }

    #[getter]
    fn end_line(&self) -> Option<u32> {
        self.inner.end_line
    }

    #[getter]
    fn end_column(&self) -> Option<u32> {
        self.inner.end_column
    }

    #[getter]
    fn metric_name(&self) -> String {
        self.inner.metric_name.clone()
    }

    #[getter]
    fn actual_value(&self) -> f64 {
        self.inner.actual_value
    }

    #[getter]
    fn threshold_value(&self) -> f64 {
        self.inner.threshold_value
    }

    #[getter]
    fn severity(&self) -> String {
        severity_label(self.inner.severity).to_string()
    }
}

#[pymethods]
impl PyThresholdResult {
    #[getter]
    fn passed(&self) -> bool {
        self.inner.passed
    }

    #[getter]
    fn violations(&self) -> Vec<PyThresholdViolation> {
        self.inner
            .violations
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }
}

#[pymethods]
impl PyProjectAnalysis {
    #[getter]
    fn files(&self) -> Vec<PyFileAnalysis> {
        self.inner.files.iter().cloned().map(Into::into).collect()
    }

    #[getter]
    fn summary(&self) -> PyProjectSummary {
        self.inner.summary.clone().into()
    }

    #[getter]
    fn threshold_violations(&self) -> Vec<PyThresholdViolation> {
        self.inner
            .threshold_violations
            .iter()
            .cloned()
            .map(Into::into)
            .collect()
    }
}

#[pymodule]
fn rivet_rs(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<PyAnalyzer>()?;
    module.add_class::<PyLanguageDescriptor>()?;
    module.add_class::<PyHalsteadMetrics>()?;
    module.add_class::<PyFileMetrics>()?;
    module.add_class::<PyFunctionAnalysis>()?;
    module.add_class::<PyPluginDiagnostic>()?;
    module.add_class::<PyParseError>()?;
    module.add_class::<PyFileAnalysis>()?;
    module.add_class::<PyLanguageSummary>()?;
    module.add_class::<PyProjectSummary>()?;
    module.add_class::<PyThresholdViolation>()?;
    module.add_class::<PyThresholdResult>()?;
    module.add_class::<PyProjectAnalysis>()?;
    Ok(())
}

impl From<HalsteadMetrics> for PyHalsteadMetrics {
    fn from(inner: HalsteadMetrics) -> Self {
        Self { inner }
    }
}

impl From<FileMetrics> for PyFileMetrics {
    fn from(inner: FileMetrics) -> Self {
        Self { inner }
    }
}

impl From<FunctionAnalysis> for PyFunctionAnalysis {
    fn from(inner: FunctionAnalysis) -> Self {
        Self { inner }
    }
}

impl From<PluginDiagnostic> for PyPluginDiagnostic {
    fn from(inner: PluginDiagnostic) -> Self {
        Self { inner }
    }
}

impl From<ParseError> for PyParseError {
    fn from(inner: ParseError) -> Self {
        Self { inner }
    }
}

impl From<FileAnalysis> for PyFileAnalysis {
    fn from(inner: FileAnalysis) -> Self {
        Self { inner }
    }
}

impl From<LanguageSummary> for PyLanguageSummary {
    fn from(inner: LanguageSummary) -> Self {
        Self { inner }
    }
}

impl From<ProjectSummary> for PyProjectSummary {
    fn from(inner: ProjectSummary) -> Self {
        Self { inner }
    }
}

impl From<ThresholdViolation> for PyThresholdViolation {
    fn from(inner: ThresholdViolation) -> Self {
        Self { inner }
    }
}

impl From<ThresholdResult> for PyThresholdResult {
    fn from(inner: ThresholdResult) -> Self {
        Self { inner }
    }
}

impl From<ProjectAnalysis> for PyProjectAnalysis {
    fn from(inner: ProjectAnalysis) -> Self {
        Self { inner }
    }
}

fn parse_language(language: &str) -> PyResult<Language> {
    match resolve_language(language).map_err(to_py_err)? {
        LanguageResolution::Full { language, .. } => Ok(language),
        LanguageResolution::ParseOnly(descriptor) => Err(to_py_err(format!(
            "recognized but parse-only language `{}`",
            descriptor.id
        ))),
    }
}

fn metric_map_to_py(py: Python<'_>, metrics: &HashMap<String, MetricValue>) -> PyResult<Py<PyAny>> {
    let dict = PyDict::new(py);
    for (name, value) in metrics {
        dict.set_item(name, metric_value_to_py(py, value)?)?;
    }
    dict.into_py_any(py)
}

fn metric_value_to_py(py: Python<'_>, value: &MetricValue) -> PyResult<Py<PyAny>> {
    match value {
        MetricValue::Integer(value) => value.into_py_any(py),
        MetricValue::Float(value) => value.into_py_any(py),
        MetricValue::Composite(values) => {
            let dict = PyDict::new(py);
            for (name, value) in values {
                dict.set_item(name, metric_value_to_py(py, value)?)?;
            }
            dict.into_py_any(py)
        }
    }
}

fn severity_label(severity: Severity) -> &'static str {
    match severity {
        Severity::Warning => "warning",
        Severity::Error => "error",
    }
}

fn to_py_err(error: impl ToString) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    fn temp_dir(name: &str) -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("rivet-python-{name}-{suffix}"));
        fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    #[test]
    fn available_languages_expose_metadata() {
        let analyzer = PyAnalyzer::new(None, None, None, None, None).expect("analyzer");
        let languages = analyzer.available_languages();

        assert!(languages.len() > analyzer.supported_languages().len());
        assert!(languages.iter().any(|language| language.id() == "rust"));
        assert!(languages.iter().any(|language| language.id() == "swift"));
        assert!(
            languages
                .iter()
                .any(|language| language.support_level() == "parse_only")
        );
    }

    #[test]
    fn parse_only_source_is_rejected_explicitly() {
        let analyzer = PyAnalyzer::new(None, None, None, None, None).expect("analyzer");
        let result =
            analyzer.analyze_source("func sample(value: Int) -> Int { value }", "swift", None);

        match result {
            Ok(_) => panic!("parse-only language should fail"),
            Err(error) => assert!(error.to_string().contains("parse-only")),
        }
    }

    #[test]
    fn analyze_directory_uses_shared_collector_for_supported_files() {
        let analyzer = PyAnalyzer::new(None, None, None, None, None).expect("analyzer");
        let dir = temp_dir("collect");
        let rust_path = dir.join("sample.rs");
        let swift_path = dir.join("sample.swift");
        fs::write(&rust_path, "fn sample() {}").expect("write rust");
        fs::write(&swift_path, "func sample(value: Int) -> Int { value }").expect("write swift");

        let dir_str = dir.display().to_string();
        let project = analyzer.analyze_directory(&dir_str, None).expect("project");

        assert_eq!(project.files().len(), 1);
        assert_eq!(project.files()[0].language(), "rust");

        fs::remove_dir_all(dir).expect("cleanup");
    }
}
