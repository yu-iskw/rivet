use std::{collections::BTreeSet, fmt::Write as _};

use serde::Serialize;

use crate::{
    error::RivetError,
    types::{ProjectAnalysis, ThresholdViolation},
};

pub fn to_json<T: Serialize>(value: &T) -> Result<String, RivetError> {
    serde_json::to_string_pretty(value)
        .map_err(|error| RivetError::Serialization(error.to_string()))
}

#[must_use]
pub fn to_text(project: &ProjectAnalysis) -> String {
    let mut lines = Vec::new();
    for file in &project.files {
        lines.push(
            file.file_path
                .as_ref()
                .map_or_else(|| "<memory>".to_string(), |path| path.display().to_string()),
        );
        for function in &file.functions {
            let mut line = format!(
                "  {} CC={} Cognitive={} Params={} NLOC={}",
                function.name,
                function.cyclomatic_complexity,
                function.cognitive_complexity,
                function.parameter_count,
                function.nloc
            );
            if !function.custom_metrics.is_empty() {
                let custom = serde_json::to_string(&function.custom_metrics)
                    .unwrap_or_else(|_| "{}".to_string());
                let _ = write!(line, " Custom={custom}");
            }
            lines.push(line);
        }
        if !file.plugin_diagnostics.is_empty() {
            lines.push(format!(
                "  Plugin diagnostics: {}",
                file.plugin_diagnostics.len()
            ));
        }
    }
    lines.push(format!(
        "Summary: {} files | {} functions | Total NLOC: {}",
        project.summary.total_files, project.summary.total_functions, project.summary.total_nloc
    ));
    if !project.threshold_violations.is_empty() {
        lines.push(format!(
            "Violations: {}",
            project.threshold_violations.len()
        ));
    }
    lines.join("\n")
}

#[must_use]
pub fn to_csv(project: &ProjectAnalysis) -> String {
    let mut rows = vec![
        "file,function,start_line,end_line,cc,cognitive,params,nloc,tokens,custom_metrics"
            .to_string(),
    ];
    for file in &project.files {
        let file_path = file
            .file_path
            .as_ref()
            .map_or_else(|| "<memory>".to_string(), |path| path.display().to_string());
        for function in &file.functions {
            rows.push(format!(
                "{},{},{},{},{},{},{},{},{},{}",
                file_path,
                function.name,
                function.start_line,
                function.end_line,
                function.cyclomatic_complexity,
                function.cognitive_complexity,
                function.parameter_count,
                function.nloc,
                function.token_count,
                serde_json::to_string(&function.custom_metrics)
                    .unwrap_or_else(|_| "{}".to_string())
            ));
        }
    }
    rows.join("\n")
}

pub fn to_sarif(project: &ProjectAnalysis) -> Result<String, RivetError> {
    let rules = project
        .threshold_violations
        .iter()
        .map(|violation| violation.metric_name.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|metric_name| {
            serde_json::json!({
                "id": metric_name,
                "name": metric_name,
                "shortDescription": { "text": format!("Threshold exceeded for {metric_name}") },
            })
        })
        .collect::<Vec<_>>();
    let results = project
        .threshold_violations
        .iter()
        .map(violation_to_sarif_result)
        .collect::<Vec<_>>();

    to_json(&serde_json::json!({
        "version": "2.1.0",
        "$schema": "https://schemastore.azurewebsites.net/schemas/json/sarif-2.1.0-rtm.5.json",
        "runs": [{
            "tool": {
                "driver": {
                    "name": "rivet",
                    "version": env!("CARGO_PKG_VERSION"),
                    "rules": rules
                }
            },
            "results": results
        }]
    }))
}

fn violation_to_sarif_result(violation: &ThresholdViolation) -> serde_json::Value {
    let region = violation.start_line.map(|start_line| {
        serde_json::json!({
            "startLine": start_line,
            "startColumn": violation.start_column.unwrap_or(1),
            "endLine": violation.end_line.unwrap_or(start_line),
            "endColumn": violation.end_column.unwrap_or(1),
        })
    });
    let mut physical_location = serde_json::json!({
        "artifactLocation": {
            "uri": violation.file_path.as_ref().map_or_else(|| "<memory>".to_string(), |path| path.display().to_string())
        }
    });
    if let Some(region) = region {
        physical_location["region"] = region;
    }

    serde_json::json!({
        "ruleId": violation.metric_name,
        "level": sarif_level(violation.severity),
        "message": {
            "text": format!(
                "{} exceeded threshold: actual={}, threshold={}",
                violation.function_name, violation.actual_value, violation.threshold_value
            )
        },
        "locations": [{
            "physicalLocation": physical_location
        }]
    })
}

const fn sarif_level(severity: crate::types::Severity) -> &'static str {
    match severity {
        crate::types::Severity::Warning => "warning",
        crate::types::Severity::Error => "error",
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeSet,
        fs,
        path::{Path, PathBuf},
    };

    use insta::{assert_json_snapshot, with_settings};
    use proptest::prelude::*;

    use super::{to_json, to_sarif};
    use crate::{
        Analyzer, AnalyzerConfig, FileInput, Language,
        types::{ProjectAnalysis, Severity, ThresholdViolation},
    };

    fn fixture_path(relative: &str) -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/fixtures")
            .join(relative)
    }

    fn all_fixture_inputs() -> Vec<FileInput> {
        [
            ("c/simple.c", Language::C),
            ("cpp/simple.cpp", Language::Cpp),
            ("csharp/Simple.cs", Language::CSharp),
            ("go/simple.go", Language::Go),
            ("java/Simple.java", Language::Java),
            ("javascript/simple.js", Language::JavaScript),
            ("kotlin/Simple.kt", Language::Kotlin),
            ("php/simple.php", Language::Php),
            ("python/simple.py", Language::Python),
            ("ruby/simple.rb", Language::Ruby),
            ("rust/simple.rs", Language::Rust),
            ("typescript/simple.ts", Language::TypeScript),
        ]
        .into_iter()
        .map(|(relative, language)| {
            let path = fixture_path(relative);
            let relative_path = Path::new("tests/fixtures").join(relative);
            FileInput {
                file_path: Some(relative_path),
                language,
                source: fs::read(path).expect("fixture source"),
            }
        })
        .collect()
    }

    fn tightened_config() -> AnalyzerConfig {
        let mut config = AnalyzerConfig::default();
        config.plugins.enabled = false;
        config.thresholds.max_cyclomatic_complexity = Some(1);
        config.thresholds.max_cognitive_complexity = Some(0);
        config.thresholds.max_function_length = Some(1);
        config.thresholds.max_parameter_count = Some(1);
        config.thresholds.max_nesting_depth = Some(0);
        config.thresholds.min_maintainability_index = Some(100.0);
        config
    }

    fn fixture_analysis() -> ProjectAnalysis {
        Analyzer::new(tightened_config())
            .expect("analyzer")
            .analyze_files(&all_fixture_inputs())
            .expect("fixture analysis")
    }

    fn path_for_language(language: Language) -> PathBuf {
        let extension = match language {
            Language::Rust => "rs",
            Language::Python => "py",
            Language::TypeScript => "ts",
            Language::JavaScript => "js",
            Language::Go => "go",
            Language::Java => "java",
            Language::C => "c",
            Language::Cpp => "cpp",
            Language::CSharp => "cs",
            Language::Ruby => "rb",
            Language::Php => "php",
            Language::Kotlin => "kt",
        };
        PathBuf::from(format!("fuzz/input.{extension}"))
    }

    #[test]
    fn sarif_includes_region_for_threshold_violations() {
        let analysis = ProjectAnalysis {
            threshold_violations: vec![ThresholdViolation {
                file_path: Some(PathBuf::from("src/lib.rs")),
                function_name: "foo".to_string(),
                start_line: Some(10),
                start_column: Some(4),
                end_line: Some(12),
                end_column: Some(8),
                metric_name: "cyclomatic_complexity".to_string(),
                actual_value: 18.0,
                threshold_value: 15.0,
                severity: Severity::Warning,
            }],
            ..ProjectAnalysis::default()
        };

        let sarif = serde_json::from_str::<serde_json::Value>(&to_sarif(&analysis).expect("sarif"))
            .expect("valid sarif json");
        let region = &sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"];
        assert_eq!(region["startLine"], 10);
        assert_eq!(region["endLine"], 12);
    }

    #[test]
    fn project_analysis_json_matches_snapshot_for_fixture_suite() {
        let analysis = serde_json::to_value(fixture_analysis()).expect("project analysis json");
        with_settings!({sort_maps => true}, {
            assert_json_snapshot!(
                "project_analysis_fixture_suite_json",
                analysis,
                {
                    ".files[].analysis_duration" => "[duration]"
                }
            );
        });
    }

    #[test]
    fn sarif_matches_snapshot_for_fixture_suite() {
        let analysis = fixture_analysis();
        let sarif = serde_json::from_str::<serde_json::Value>(&to_sarif(&analysis).expect("sarif"))
            .expect("valid sarif");
        with_settings!({sort_maps => true}, {
            assert_json_snapshot!("project_analysis_fixture_suite_sarif", sarif);
        });
    }

    #[test]
    fn sarif_fixture_suite_has_consistent_driver_and_result_metadata() {
        let analysis = fixture_analysis();
        let sarif = serde_json::from_str::<serde_json::Value>(&to_sarif(&analysis).expect("sarif"))
            .expect("valid sarif json");
        let rules = sarif["runs"][0]["tool"]["driver"]["rules"]
            .as_array()
            .expect("rules array");
        let results = sarif["runs"][0]["results"]
            .as_array()
            .expect("results array");
        let rule_ids = rules
            .iter()
            .filter_map(|rule| rule["id"].as_str())
            .collect::<BTreeSet<_>>();
        let artifact_uris = results
            .iter()
            .filter_map(|result| {
                result["locations"][0]["physicalLocation"]["artifactLocation"]["uri"].as_str()
            })
            .collect::<BTreeSet<_>>();

        assert_eq!(results.len(), analysis.threshold_violations.len());
        assert_eq!(artifact_uris.len(), all_fixture_inputs().len());
        assert!(
            results
                .iter()
                .all(|result| result["message"]["text"].is_string())
        );
        assert!(results.iter().all(|result| {
            result["ruleId"]
                .as_str()
                .is_some_and(|rule_id| rule_ids.contains(rule_id))
        }));
    }

    #[test]
    fn json_output_round_trips_for_fixture_suite() {
        let analysis = fixture_analysis();
        let rendered = to_json(&analysis).expect("json output");
        let reparsed = serde_json::from_str::<serde_json::Value>(&rendered).expect("parse json");
        assert_eq!(
            reparsed["summary"]["total_files"].as_u64(),
            Some(all_fixture_inputs().len() as u64)
        );
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 24,
            max_shrink_iters: 0,
            .. ProptestConfig::default()
        })]

        #[test]
        fn malformed_inputs_do_not_panic_for_supported_languages(
            source in prop::collection::vec(any::<u8>(), 0..96),
            language in prop_oneof![
                Just(Language::Rust),
                Just(Language::Python),
                Just(Language::TypeScript),
                Just(Language::JavaScript),
                Just(Language::Go),
                Just(Language::Java),
                Just(Language::C),
                Just(Language::Cpp),
                Just(Language::CSharp),
                Just(Language::Ruby),
                Just(Language::Php),
                Just(Language::Kotlin),
            ],
        ) {
            let mut config = AnalyzerConfig::default();
            config.plugins.enabled = false;
            let analyzer = Analyzer::new(config).expect("analyzer");
            let path = path_for_language(language);
            let result = analyzer.analyze_source(&source, language, Some(&path));

            if let Ok(analysis) = result {
                prop_assert!(analysis.functions.iter().all(|function| function.end_line >= function.start_line));
                prop_assert!(analysis.parse_errors.iter().all(|error| error.end_line >= error.start_line));
            }
        }
    }
}
