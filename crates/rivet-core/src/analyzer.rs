#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::elidable_lifetime_names,
    clippy::missing_const_for_fn,
    clippy::suboptimal_flops
)]

use std::time::Instant;

use crate::{
    config::AnalyzerConfig,
    error::RivetError,
    language::{Language, LanguageRegistry},
    metrics::{
        compute_cognitive_complexity, compute_cyclomatic_complexity, compute_file_metrics,
        compute_function_nloc, compute_halstead, compute_nesting_depth, total_complexity,
    },
    parser::Parser,
    plugin::PluginHost,
    types::{
        FileAnalysis, FileInput, FunctionAnalysis, LanguageSummary, ProjectAnalysis,
        ProjectSummary, ThresholdResult, ThresholdViolation,
    },
};
use rayon::prelude::*;

pub struct Analyzer {
    language_registry: LanguageRegistry,
    plugin_host: Option<PluginHost>,
    config: AnalyzerConfig,
}

impl Analyzer {
    pub fn new(config: AnalyzerConfig) -> Result<Self, RivetError> {
        Ok(Self {
            language_registry: LanguageRegistry::new()?,
            plugin_host: None,
            config,
        })
    }

    pub fn analyze_source(
        &self,
        source: &[u8],
        language: Language,
        file_path: Option<&std::path::Path>,
    ) -> Result<FileAnalysis, RivetError> {
        let started_at = Instant::now();
        let language_config = self.language_registry.get(language)?;
        let mut parser = Parser::new();
        let parsed = parser.parse(source, language_config)?;
        let mut file_metrics = compute_file_metrics(source, &language_config.comment_prefixes);
        file_metrics.halstead = compute_halstead(parsed.tree.root_node(), source, language_config);

        let mut functions = Vec::new();
        for node in collect_function_nodes(parsed.tree.root_node()) {
            let Some(name_node) = node.child_by_field_name("name") else {
                continue;
            };
            let Ok(function_name) = name_node.utf8_text(source) else {
                continue;
            };

            let start = node.start_position();
            let end = node.end_position();
            let start_line = start.row as u32 + 1;
            let end_line = end.row as u32 + 1;
            let halstead = compute_halstead(node, source, language_config);

            functions.push(FunctionAnalysis {
                name: function_name.to_string(),
                qualified_name: function_name.to_string(),
                start_line,
                end_line,
                start_column: start.column as u32,
                end_column: end.column as u32,
                cyclomatic_complexity: compute_cyclomatic_complexity(node, source, language_config),
                cognitive_complexity: compute_cognitive_complexity(node, source, language_config),
                parameter_count: parameter_count(node),
                token_count: node.named_child_count() as u32,
                nloc: compute_function_nloc(source, start_line, end_line),
                halstead,
                nesting_depth: compute_nesting_depth(node, source, language_config),
            });
        }

        let (total_complexity, avg_complexity, max_complexity) = total_complexity(&functions);
        file_metrics.total_complexity = total_complexity;
        file_metrics.avg_complexity = avg_complexity;
        file_metrics.max_complexity = max_complexity;
        file_metrics.maintainability_index = maintainability_index(
            file_metrics.halstead.volume,
            total_complexity,
            file_metrics.sloc,
        );

        Ok(FileAnalysis {
            file_path: file_path.map(std::path::Path::to_path_buf),
            language,
            file_metrics,
            functions,
            parse_errors: parsed.errors,
            analysis_duration: started_at.elapsed(),
        })
    }

    pub fn analyze_files(&self, files: &[FileInput]) -> Result<ProjectAnalysis, RivetError> {
        let analyses = files
            .par_iter()
            .map(|file| self.analyze_source(&file.source, file.language, file.file_path.as_deref()))
            .collect::<Result<Vec<_>, _>>()?;
        let threshold_violations = analyses
            .iter()
            .flat_map(|analysis| self.check_file_thresholds(analysis))
            .collect::<Vec<_>>();

        Ok(ProjectAnalysis {
            summary: build_summary(&analyses),
            files: analyses,
            threshold_violations,
        })
    }

    #[must_use]
    pub fn check_thresholds(&self, analysis: &ProjectAnalysis) -> ThresholdResult {
        ThresholdResult {
            passed: analysis.threshold_violations.is_empty(),
            violations: analysis.threshold_violations.clone(),
        }
    }

    pub fn register_plugin(&mut self, _wasm_bytes: &[u8]) -> Result<(), RivetError> {
        if self.plugin_host.is_none() {
            self.plugin_host = Some(PluginHost);
        }
        Ok(())
    }

    #[must_use]
    pub fn supported_languages(&self) -> Vec<Language> {
        self.language_registry.supported_languages()
    }

    fn check_file_thresholds(&self, analysis: &FileAnalysis) -> Vec<ThresholdViolation> {
        let mut violations = Vec::new();

        for function in &analysis.functions {
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                &function.name,
                "cyclomatic_complexity",
                f64::from(function.cyclomatic_complexity),
                self.config
                    .thresholds
                    .max_cyclomatic_complexity
                    .map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                &function.name,
                "cognitive_complexity",
                f64::from(function.cognitive_complexity),
                self.config
                    .thresholds
                    .max_cognitive_complexity
                    .map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                &function.name,
                "function_length",
                f64::from(function.nloc),
                self.config.thresholds.max_function_length.map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                &function.name,
                "parameter_count",
                f64::from(function.parameter_count),
                self.config.thresholds.max_parameter_count.map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                &function.name,
                "nesting_depth",
                f64::from(function.nesting_depth),
                self.config.thresholds.max_nesting_depth.map(f64::from),
            );
        }

        if let Some(threshold) = self.config.thresholds.min_maintainability_index
            && analysis.file_metrics.maintainability_index < threshold
        {
            violations.push(ThresholdViolation {
                file_path: analysis.file_path.clone(),
                function_name: "<file>".to_string(),
                metric_name: "maintainability_index".to_string(),
                actual_value: analysis.file_metrics.maintainability_index,
                threshold_value: threshold,
                severity: crate::types::Severity::Warning,
            });
        }

        violations
    }
}

fn parameter_count(node: tree_sitter::Node<'_>) -> u32 {
    let mut cursor = node.walk();
    node.children(&mut cursor)
        .find(|child| child.kind().contains("parameter"))
        .map_or(0, |parameters| parameters.named_child_count() as u32)
}

fn collect_function_nodes<'a>(root: tree_sitter::Node<'a>) -> Vec<tree_sitter::Node<'a>> {
    let mut result = Vec::new();
    collect_function_nodes_inner(root, &mut result);
    result
}

fn collect_function_nodes_inner<'a>(
    node: tree_sitter::Node<'a>,
    result: &mut Vec<tree_sitter::Node<'a>>,
) {
    if is_function_node(node) {
        result.push(node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_function_nodes_inner(child, result);
    }
}

fn is_function_node(node: tree_sitter::Node<'_>) -> bool {
    matches!(
        node.kind(),
        "function_item" | "function_definition" | "method_definition" | "method_declaration"
    )
}

fn build_summary(files: &[FileAnalysis]) -> ProjectSummary {
    let mut languages = std::collections::HashMap::new();
    let total_files = files.len() as u32;
    let total_functions = files.iter().map(|file| file.functions.len() as u32).sum();
    let total_nloc = files.iter().map(|file| file.file_metrics.nloc).sum();
    let avg_cyclomatic = average(files.iter().flat_map(|file| {
        file.functions
            .iter()
            .map(|function| f64::from(function.cyclomatic_complexity))
    }));
    let avg_cognitive = average(files.iter().flat_map(|file| {
        file.functions
            .iter()
            .map(|function| f64::from(function.cognitive_complexity))
    }));
    let avg_maintainability_index = average(
        files
            .iter()
            .map(|file| file.file_metrics.maintainability_index),
    );

    for file in files {
        let entry = languages
            .entry(file.language)
            .or_insert_with(LanguageSummary::default);
        entry.files += 1;
        entry.functions += file.functions.len() as u32;
        entry.nloc += file.file_metrics.nloc;
    }

    ProjectSummary {
        total_files,
        total_functions,
        total_nloc,
        avg_cyclomatic,
        avg_cognitive,
        avg_maintainability_index,
        languages,
    }
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let values = values.collect::<Vec<_>>();
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

fn maintainability_index(volume: f64, cyclomatic: f64, sloc: u32) -> f64 {
    if volume <= 0.0 || sloc == 0 {
        return 100.0;
    }

    let score =
        (171.0 - 5.2 * volume.ln() - 0.23 * cyclomatic - 16.2 * (sloc as f64).ln()) * 100.0 / 171.0;
    score.clamp(0.0, 100.0)
}

fn push_limit_violation(
    violations: &mut Vec<ThresholdViolation>,
    file_path: Option<std::path::PathBuf>,
    function_name: &str,
    metric_name: &str,
    actual_value: f64,
    threshold: Option<f64>,
) {
    if let Some(threshold_value) = threshold
        && actual_value > threshold_value
    {
        violations.push(ThresholdViolation {
            file_path,
            function_name: function_name.to_string(),
            metric_name: metric_name.to_string(),
            actual_value,
            threshold_value,
            severity: crate::types::Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use crate::{Analyzer, AnalyzerConfig, Language};

    #[test]
    fn analyzes_rust_function() {
        let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let result = analyzer
            .analyze_source(
                b"fn foo(x: i32) -> i32 { if x > 0 { x } else { -x } }",
                Language::Rust,
                None,
            )
            .expect("analysis");

        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.functions[0].name, "foo");
        assert_eq!(result.functions[0].cyclomatic_complexity, 3);
    }

    #[test]
    fn analyzes_python_function() {
        let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let result = analyzer
            .analyze_source(
                b"def foo(x):\n    if x > 0:\n        return x\n    return -x\n",
                Language::Python,
                None,
            )
            .expect("analysis");

        assert_eq!(result.functions.len(), 1);
        assert_eq!(result.functions[0].name, "foo");
        assert!(result.functions[0].cyclomatic_complexity >= 2);
    }
}
