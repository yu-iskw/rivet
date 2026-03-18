#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_lossless,
    clippy::elidable_lifetime_names,
    clippy::missing_const_for_fn,
    clippy::suboptimal_flops
)]

use std::{borrow::Cow, collections::HashMap, time::Instant};

use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::{
    config::{AnalyzerConfig, PathThresholdOverride},
    error::RivetError,
    language::{Language, LanguageRegistry},
    metrics::{
        compute_cognitive_complexity, compute_cyclomatic_complexity, compute_file_metrics,
        compute_function_nloc, compute_halstead, compute_nesting_depth, total_complexity,
    },
    parser::Parser,
    plugin::{PluginAnalysisInput, PluginHost},
    types::{
        FileAnalysis, FileInput, FunctionAnalysis, LanguageSummary, ProjectAnalysis,
        ProjectSummary, ThresholdResult, ThresholdViolation,
    },
};
use rayon::prelude::*;
use tree_sitter::{QueryCursor, StreamingIterator};

struct FunctionDescriptor<'tree> {
    node: tree_sitter::Node<'tree>,
    name: String,
    parameter_count: u32,
}

pub struct Analyzer {
    language_registry: LanguageRegistry,
    #[allow(dead_code)]
    plugin_host: Option<PluginHost>,
    config: AnalyzerConfig,
    threshold_overrides: Vec<CompiledThresholdOverride>,
}

struct CompiledThresholdOverride {
    matcher: GlobSet,
    thresholds: crate::types::Thresholds,
}

impl Analyzer {
    pub fn new(config: AnalyzerConfig) -> Result<Self, RivetError> {
        let threshold_overrides = config
            .threshold_overrides
            .iter()
            .map(compile_threshold_override)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            language_registry: LanguageRegistry::new()?,
            plugin_host: config
                .plugins
                .enabled
                .then(|| PluginHost::new(config.plugins.clone())),
            config,
            threshold_overrides,
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

        let function_descriptors =
            collect_functions(parsed.tree.root_node(), source, language, language_config)?;
        let mut functions = Vec::with_capacity(function_descriptors.len());
        let mut plugin_diagnostics = Vec::new();

        for function in function_descriptors {
            let node = function.node;
            let start = node.start_position();
            let end = node.end_position();
            let start_line = start.row as u32 + 1;
            let end_line = end.row as u32 + 1;
            let halstead = compute_halstead(node, source, language_config);
            let (custom_metrics, function_plugin_diagnostics) =
                self.plugin_host.as_ref().map_or_else(
                    || (HashMap::new(), Vec::new()),
                    |host| {
                        host.analyze_function(&PluginAnalysisInput {
                            source: String::from_utf8_lossy(&source[node.byte_range()])
                                .into_owned(),
                            function_name: function.name.clone(),
                            language: language.as_str().to_string(),
                            sexp: node.to_sexp(),
                            start_line,
                            end_line,
                        })
                    },
                );
            plugin_diagnostics.extend(function_plugin_diagnostics);

            functions.push(FunctionAnalysis {
                name: function.name.clone(),
                qualified_name: function.name,
                start_line,
                end_line,
                start_column: start.column as u32,
                end_column: end.column as u32,
                cyclomatic_complexity: compute_cyclomatic_complexity(node, source, language_config),
                cognitive_complexity: compute_cognitive_complexity(node, source, language_config),
                parameter_count: function.parameter_count,
                token_count: node.named_child_count() as u32,
                nloc: compute_function_nloc(source, start_line, end_line),
                halstead,
                nesting_depth: compute_nesting_depth(node, source, language_config),
                custom_metrics,
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
            plugin_diagnostics,
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

    pub fn register_plugin(&mut self, wasm_bytes: &[u8]) -> Result<(), RivetError> {
        self.plugin_host
            .get_or_insert_with(|| PluginHost::new(self.config.plugins.clone()))
            .register_plugin(wasm_bytes)
    }

    #[must_use]
    pub fn supported_languages(&self) -> Vec<Language> {
        self.language_registry.supported_languages()
    }

    #[must_use]
    pub fn available_languages(&self) -> Vec<crate::language::LanguageDescriptor> {
        self.language_registry.available_languages()
    }

    #[must_use]
    pub fn check_file_thresholds(&self, analysis: &FileAnalysis) -> Vec<ThresholdViolation> {
        let thresholds = self.thresholds_for_path(analysis.file_path.as_deref());
        let mut violations = Vec::new();

        for function in &analysis.functions {
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                function,
                "cyclomatic_complexity",
                f64::from(function.cyclomatic_complexity),
                thresholds.max_cyclomatic_complexity.map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                function,
                "cognitive_complexity",
                f64::from(function.cognitive_complexity),
                thresholds.max_cognitive_complexity.map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                function,
                "function_length",
                f64::from(function.nloc),
                thresholds.max_function_length.map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                function,
                "parameter_count",
                f64::from(function.parameter_count),
                thresholds.max_parameter_count.map(f64::from),
            );
            push_limit_violation(
                &mut violations,
                analysis.file_path.clone(),
                function,
                "nesting_depth",
                f64::from(function.nesting_depth),
                thresholds.max_nesting_depth.map(f64::from),
            );
        }

        if let Some(threshold) = thresholds.min_maintainability_index
            && analysis.file_metrics.maintainability_index < threshold
        {
            violations.push(ThresholdViolation {
                file_path: analysis.file_path.clone(),
                function_name: "<file>".to_string(),
                start_line: None,
                start_column: None,
                end_line: None,
                end_column: None,
                metric_name: "maintainability_index".to_string(),
                actual_value: analysis.file_metrics.maintainability_index,
                threshold_value: threshold,
                severity: crate::types::Severity::Warning,
            });
        }

        violations
    }

    fn thresholds_for_path(&self, file_path: Option<&std::path::Path>) -> crate::types::Thresholds {
        let mut thresholds = self.config.thresholds.clone();
        let Some(path) = file_path else {
            return thresholds;
        };
        let path = path.to_string_lossy();
        for override_entry in &self.threshold_overrides {
            if override_entry.matcher.is_match(path.as_ref())
                || override_entry.matcher.is_match(format!("./{path}"))
            {
                merge_thresholds(&mut thresholds, &override_entry.thresholds);
            }
        }
        thresholds
    }
}

fn compile_threshold_override(
    threshold_override: &PathThresholdOverride,
) -> Result<CompiledThresholdOverride, RivetError> {
    let mut builder = GlobSetBuilder::new();
    for pattern in &threshold_override.paths {
        let glob = Glob::new(pattern).map_err(|error| {
            RivetError::Analysis(format!("invalid threshold override: {error}"))
        })?;
        builder.add(glob);
    }

    let matcher = builder
        .build()
        .map_err(|error| RivetError::Analysis(format!("invalid threshold override: {error}")))?;

    Ok(CompiledThresholdOverride {
        matcher,
        thresholds: threshold_override.thresholds.clone(),
    })
}

fn merge_thresholds(target: &mut crate::types::Thresholds, source: &crate::types::Thresholds) {
    if source.max_cyclomatic_complexity.is_some() {
        target.max_cyclomatic_complexity = source.max_cyclomatic_complexity;
    }
    if source.max_cognitive_complexity.is_some() {
        target.max_cognitive_complexity = source.max_cognitive_complexity;
    }
    if source.max_function_length.is_some() {
        target.max_function_length = source.max_function_length;
    }
    if source.max_parameter_count.is_some() {
        target.max_parameter_count = source.max_parameter_count;
    }
    if source.max_nesting_depth.is_some() {
        target.max_nesting_depth = source.max_nesting_depth;
    }
    if source.min_maintainability_index.is_some() {
        target.min_maintainability_index = source.min_maintainability_index;
    }
}

fn missing_query_capture(language: Language, capture: &str) -> RivetError {
    RivetError::MissingQueryCapture {
        language: language.as_str().to_string(),
        capture: capture.to_string(),
    }
}

fn canonical_capture_name(name: &str) -> Cow<'_, str> {
    if name.contains('.') {
        Cow::Owned(name.replace('.', "_"))
    } else {
        Cow::Borrowed(name)
    }
}

fn collect_functions<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
    language: Language,
    language_config: &crate::language::LanguageConfig,
) -> Result<Vec<FunctionDescriptor<'a>>, RivetError> {
    collect_functions_from_query(root, source, language, language_config)
}

fn collect_functions_from_query<'a>(
    root: tree_sitter::Node<'a>,
    source: &[u8],
    language: Language,
    language_config: &crate::language::LanguageConfig,
) -> Result<Vec<FunctionDescriptor<'a>>, RivetError> {
    let capture_names = language_config.function_query.capture_names();
    let mut cursor = QueryCursor::new();
    let mut result = Vec::new();

    let mut matches = cursor.matches(&language_config.function_query, root, source);
    matches.advance();
    while let Some(query_match) = matches.get() {
        let mut function_node = None;
        let mut function_name = None;
        let mut parameter_node = None;

        for capture in query_match.captures {
            let capture_name = canonical_capture_name(capture_names[capture.index as usize]);
            match capture_name.as_ref() {
                "function_def" => function_node = Some(capture.node),
                "function_name" => {
                    function_name = capture.node.utf8_text(source).ok().map(str::to_owned);
                }
                "function_parameters" => {
                    parameter_node = Some(capture.node);
                }
                _ => {}
            }
        }

        let node = function_node.ok_or_else(|| missing_query_capture(language, "function_def"))?;
        let name = function_name.ok_or_else(|| missing_query_capture(language, "function_name"))?;
        let params =
            parameter_node.ok_or_else(|| missing_query_capture(language, "function_parameters"))?;
        let parameter_count = params.named_child_count() as u32;

        result.push(FunctionDescriptor {
            node,
            name,
            parameter_count,
        });
        matches.advance();
    }

    Ok(result)
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
    function: &FunctionAnalysis,
    metric_name: &str,
    actual_value: f64,
    threshold: Option<f64>,
) {
    if let Some(threshold_value) = threshold
        && actual_value > threshold_value
    {
        violations.push(ThresholdViolation {
            file_path,
            function_name: function.name.clone(),
            start_line: Some(function.start_line),
            start_column: Some(function.start_column),
            end_line: Some(function.end_line),
            end_column: Some(function.end_column),
            metric_name: metric_name.to_string(),
            actual_value,
            threshold_value,
            severity: crate::types::Severity::Warning,
        });
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::collect_functions_from_query;
    use crate::{
        Analyzer, AnalyzerConfig, Language, PathThresholdOverride, RivetError, Thresholds,
    };

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
        assert_eq!(result.functions[0].parameter_count, 1);
        assert!(result.functions[0].cyclomatic_complexity >= 2);
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
        assert_eq!(result.functions[0].parameter_count, 1);
        assert!(result.functions[0].cyclomatic_complexity >= 2);
    }

    #[allow(clippy::too_many_lines)]
    #[test]
    fn analyzes_all_language_fixtures() {
        let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let fixtures = [
            (
                "../../tests/fixtures/rust/simple.rs",
                Language::Rust,
                "absolute",
                1,
            ),
            (
                "../../tests/fixtures/python/simple.py",
                Language::Python,
                "absolute",
                1,
            ),
            (
                "../../tests/fixtures/typescript/simple.ts",
                Language::TypeScript,
                "sample",
                2,
            ),
            (
                "../../tests/fixtures/javascript/simple.js",
                Language::JavaScript,
                "sample",
                2,
            ),
            (
                "../../tests/fixtures/go/simple.go",
                Language::Go,
                "sample",
                2,
            ),
            (
                "../../tests/fixtures/java/Simple.java",
                Language::Java,
                "sample",
                2,
            ),
            ("../../tests/fixtures/c/simple.c", Language::C, "sample", 2),
            (
                "../../tests/fixtures/cpp/simple.cpp",
                Language::Cpp,
                "sample",
                2,
            ),
            (
                "../../tests/fixtures/csharp/Simple.cs",
                Language::CSharp,
                "Sample",
                2,
            ),
            (
                "../../tests/fixtures/ruby/simple.rb",
                Language::Ruby,
                "sample",
                2,
            ),
            (
                "../../tests/fixtures/php/simple.php",
                Language::Php,
                "sample",
                2,
            ),
            (
                "../../tests/fixtures/kotlin/Simple.kt",
                Language::Kotlin,
                "sample",
                2,
            ),
        ];

        for (fixture, language, function_name, parameter_count) in fixtures {
            let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR")).join(fixture);
            let source = fs::read(&fixture_path).unwrap_or_else(|error| {
                panic!("failed to read {}: {error}", fixture_path.display())
            });

            let analysis = analyzer
                .analyze_source(&source, language, Some(&fixture_path))
                .unwrap_or_else(|error| panic!("analysis failed: {error}"));
            assert!(
                analysis.parse_errors.is_empty(),
                "unexpected parse errors for {}",
                fixture_path.display()
            );
            assert_eq!(
                analysis.functions.len(),
                1,
                "expected exactly one function in {}",
                fixture_path.display()
            );
            assert_eq!(analysis.functions[0].name, function_name);
            assert_eq!(analysis.functions[0].parameter_count, parameter_count);
            assert!(
                analysis.functions[0].cyclomatic_complexity >= 1,
                "expected branching complexity in {}",
                fixture_path.display()
            );
            assert!(
                analysis.functions[0].nloc >= 2,
                "expected function body lines in {}",
                fixture_path.display()
            );
        }
    }

    #[test]
    fn missing_function_parameter_capture_is_reported() {
        let grammar: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
        let source = b"fn sample(value: i32) -> i32 { value }";
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&grammar).expect("rust parser language");
        let tree = parser.parse(source, None).expect("rust parse tree");
        let root = tree.root_node();
        let function_query = tree_sitter::Query::new(
            &grammar,
            "(function_item name: (identifier) @function_name) @function_def",
        )
        .expect("function query");
        let control_flow_query = tree_sitter::Query::new(&grammar, "(if_expression) @control_flow")
            .expect("control flow query");
        let operator_query =
            tree_sitter::Query::new(&grammar, "(identifier) @operator").expect("operator query");
        let operand_query =
            tree_sitter::Query::new(&grammar, "(identifier) @operand").expect("operand query");
        let language_config = crate::language::LanguageConfig {
            grammar,
            function_query,
            control_flow_query,
            operator_query,
            operand_query,
            comment_prefixes: vec!["//"],
        };

        match collect_functions_from_query(root, source, Language::Rust, &language_config) {
            Err(RivetError::MissingQueryCapture { capture, .. }) => {
                assert_eq!(capture, "function_parameters");
            }
            Err(other) => panic!("unexpected error: {other}"),
            Ok(_) => panic!("expected missing capture error"),
        }
    }

    #[test]
    fn register_plugin_is_explicitly_unsupported() {
        let mut analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let error = analyzer
            .register_plugin(b"\0asm")
            .expect_err("invalid plugin registration should not silently succeed");

        #[cfg(feature = "plugins")]
        assert!(matches!(error, RivetError::Plugin(_)));

        #[cfg(not(feature = "plugins"))]
        assert!(matches!(error, RivetError::UnsupportedFeature(_)));
    }

    #[test]
    fn available_languages_match_supported_languages_for_popular_slice() {
        let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let supported_ids = analyzer
            .supported_languages()
            .into_iter()
            .map(|language| language.as_str().to_string())
            .collect::<Vec<_>>();
        let available = analyzer.available_languages();
        let available_ids = available
            .iter()
            .map(|descriptor| descriptor.id.clone())
            .collect::<Vec<_>>();

        for supported_id in supported_ids {
            assert!(available_ids.contains(&supported_id));
        }
        #[cfg(feature = "lang-all")]
        assert!(available.iter().any(|descriptor| {
            descriptor.support_level == crate::language::LanguageSupportLevel::ParseOnly
        }));
        #[cfg(not(feature = "lang-all"))]
        assert!(available.iter().all(|descriptor| {
            descriptor.support_level == crate::language::LanguageSupportLevel::Full
        }));
    }

    #[test]
    fn applies_path_specific_threshold_overrides() {
        let mut config = AnalyzerConfig::default();
        config.thresholds.max_cyclomatic_complexity = Some(0);
        config.threshold_overrides = vec![PathThresholdOverride {
            paths: vec!["src/tests/**".to_string()],
            thresholds: Thresholds {
                max_cyclomatic_complexity: Some(100),
                max_cognitive_complexity: None,
                max_function_length: None,
                max_parameter_count: None,
                max_nesting_depth: None,
                min_maintainability_index: None,
            },
        }];
        let analyzer = Analyzer::new(config).expect("analyzer");

        let source = br"
            int foo(int x) {
                if (x == 0) { return 0; }
                if (x == 1) { return 1; }
                if (x == 2) { return 2; }
                if (x == 3) { return 3; }
                if (x == 4) { return 4; }
                if (x == 5) { return 5; }
                if (x == 6) { return 6; }
                if (x == 7) { return 7; }
                if (x == 8) { return 8; }
                if (x == 9) { return 9; }
                if (x == 10) { return 10; }
                if (x == 11) { return 11; }
                if (x == 12) { return 12; }
                if (x == 13) { return 13; }
                if (x == 14) { return 14; }
                if (x == 15) { return 15; }
                return x;
            }
        ";
        let regular = analyzer
            .analyze_source(source, Language::C, Some(Path::new("src/lib.c")))
            .expect("regular analysis");
        let test_file = analyzer
            .analyze_source(source, Language::C, Some(Path::new("src/tests/lib.c")))
            .expect("test analysis");

        assert!(
            !analyzer.check_file_thresholds(&regular).is_empty(),
            "default thresholds should still apply"
        );
        assert!(
            analyzer
                .check_file_thresholds(&test_file)
                .iter()
                .all(|violation| violation.metric_name != "cyclomatic_complexity"),
            "override should relax cyclomatic complexity for matching paths"
        );
    }
}
