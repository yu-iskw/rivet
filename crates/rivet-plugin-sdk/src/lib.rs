use std::collections::HashMap;

use extism_pdk::{Error, FnResult, plugin_fn};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize)]
pub struct AnalyzeInput {
    pub source: String,
    pub function_name: String,
    pub language: String,
    pub sexp: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub metrics: Vec<String>,
    pub supported_languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AnalyzeOutput {
    pub metric_id: String,
    pub display_name: String,
    pub value: MetricValue,
}

impl AnalyzeOutput {
    #[must_use]
    pub fn integer(
        metric_id: impl Into<String>,
        display_name: impl Into<String>,
        value: i64,
    ) -> Self {
        Self {
            metric_id: metric_id.into(),
            display_name: display_name.into(),
            value: MetricValue::Integer(value),
        }
    }

    #[must_use]
    pub fn float(
        metric_id: impl Into<String>,
        display_name: impl Into<String>,
        value: f64,
    ) -> Self {
        Self {
            metric_id: metric_id.into(),
            display_name: display_name.into(),
            value: MetricValue::Float(value),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Float(f64),
    Composite(HashMap<String, Self>),
}

pub fn respond<T: Serialize + ?Sized>(value: &T) -> FnResult<String> {
    Ok(serde_json::to_string(value).map_err(Error::msg)?)
}

pub fn respond_outputs(outputs: &[AnalyzeOutput]) -> FnResult<String> {
    respond(outputs)
}

pub fn respond_manifest(manifest: &PluginManifest) -> FnResult<String> {
    respond(manifest)
}

pub fn parse_input(input: &str) -> Result<AnalyzeInput, Error> {
    serde_json::from_str(input).map_err(Error::msg)
}

pub fn handle_plugin<T, H>(raw: &str, handler: H) -> FnResult<String>
where
    H: FnOnce(&AnalyzeInput) -> Result<T, Error>,
    T: Serialize,
{
    let parsed = parse_input(raw)?;
    let output = handler(&parsed)?;
    respond(&output)
}

#[plugin_fn]
pub fn analyze(_input: String) -> FnResult<String> {
    Err(Error::msg("implement your plugin handler and call rivet_plugin_sdk::handle_plugin").into())
}

#[plugin_fn]
pub fn manifest() -> FnResult<String> {
    Err(
        Error::msg("implement your plugin manifest and call rivet_plugin_sdk::respond_manifest")
            .into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_input_round_trips_expected_fields() {
        let parsed = parse_input(
            r#"{
                "source":"fn demo() {}",
                "function_name":"demo",
                "language":"rust",
                "sexp":"(function_item)",
                "start_line":1,
                "end_line":1
            }"#,
        )
        .expect("input should parse");

        assert_eq!(parsed.function_name, "demo");
        assert_eq!(parsed.language, "rust");
        assert_eq!(parsed.start_line, 1);
    }

    #[test]
    fn handle_plugin_serializes_handler_output() {
        let response = handle_plugin(
            r#"{
                "source":"fn demo() {}",
                "function_name":"demo",
                "language":"rust",
                "sexp":"(function_item)",
                "start_line":1,
                "end_line":1
            }"#,
            |input| {
                Ok(vec![AnalyzeOutput {
                    metric_id: "function_name_length".to_string(),
                    display_name: "Function Name Length".to_string(),
                    value: MetricValue::Integer(
                        i64::try_from(input.function_name.len()).expect("length should fit in i64"),
                    ),
                }])
            },
        )
        .expect("handler should succeed");
        let payload: Vec<serde_json::Value> =
            serde_json::from_str(&response).expect("response JSON should parse");

        assert_eq!(payload[0]["metric_id"], "function_name_length");
        assert_eq!(payload[0]["value"], 4);
    }

    #[test]
    fn constructors_build_expected_metric_shapes() {
        let integer = AnalyzeOutput::integer("name_length", "Name Length", 4);
        let float = AnalyzeOutput::float("mi", "Maintainability", 75.5);

        assert!(matches!(integer.value, MetricValue::Integer(4)));
        assert!(
            matches!(float.value, MetricValue::Float(value) if (value - 75.5).abs() < f64::EPSILON)
        );
    }

    #[test]
    fn manifest_response_round_trips() {
        let manifest = PluginManifest {
            name: "example".to_string(),
            version: "0.1.0".to_string(),
            metrics: vec!["function_name_length".to_string()],
            supported_languages: vec!["rust".to_string()],
        };
        let response = respond_manifest(&manifest).expect("manifest response should serialize");
        let decoded =
            serde_json::from_str::<PluginManifest>(&response).expect("manifest response is valid");

        assert_eq!(decoded, manifest);
    }
}
