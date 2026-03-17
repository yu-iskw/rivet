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
            lines.push(format!(
                "  {} CC={} Cognitive={} Params={} NLOC={}",
                function.name,
                function.cyclomatic_complexity,
                function.cognitive_complexity,
                function.parameter_count,
                function.nloc
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
    let mut rows =
        vec!["file,function,start_line,end_line,cc,cognitive,params,nloc,tokens".to_string()];
    for file in &project.files {
        let file_path = file
            .file_path
            .as_ref()
            .map_or_else(|| "<memory>".to_string(), |path| path.display().to_string());
        for function in &file.functions {
            rows.push(format!(
                "{},{},{},{},{},{},{},{},{}",
                file_path,
                function.name,
                function.start_line,
                function.end_line,
                function.cyclomatic_complexity,
                function.cognitive_complexity,
                function.parameter_count,
                function.nloc,
                function.token_count
            ));
        }
    }
    rows.join("\n")
}

pub fn to_sarif(project: &ProjectAnalysis) -> Result<String, RivetError> {
    let rules = project
        .threshold_violations
        .iter()
        .map(|violation| serde_json::json!({
            "id": violation.metric_name,
            "name": violation.metric_name,
            "shortDescription": { "text": format!("Threshold exceeded for {}", violation.metric_name) },
        }))
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
    serde_json::json!({
        "ruleId": violation.metric_name,
        "level": "warning",
        "message": {
            "text": format!(
                "{} exceeded threshold: actual={}, threshold={}",
                violation.function_name, violation.actual_value, violation.threshold_value
            )
        },
        "locations": [{
            "physicalLocation": {
                "artifactLocation": {
                    "uri": violation.file_path.as_ref().map_or_else(|| "<memory>".to_string(), |path| path.display().to_string())
                }
            }
        }]
    })
}
