use extism_pdk::Error;
use rivet_plugin_sdk::{AnalyzeOutput, PluginManifest, handle_plugin, respond_manifest};

#[extism_pdk::plugin_fn]
pub fn analyze(input: String) -> extism_pdk::FnResult<String> {
    handle_plugin(&input, |payload| {
        let length = i64::try_from(payload.function_name.len())
            .map_err(|error| Error::msg(error.to_string()))?;
        Ok(vec![AnalyzeOutput::integer(
            "function_name_length",
            "Function Name Length",
            length,
        )])
    })
}

#[extism_pdk::plugin_fn]
pub fn manifest() -> extism_pdk::FnResult<String> {
    respond_manifest(&PluginManifest {
        name: "function_name_length".to_string(),
        version: "0.1.0".to_string(),
        metrics: vec!["function_name_length".to_string()],
        supported_languages: Vec::new(),
    })
}
