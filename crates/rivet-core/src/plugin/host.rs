#[cfg(feature = "plugins")]
use std::time::Duration;

#[cfg(feature = "plugins")]
use extism::{Manifest, PluginBuilder, Wasm};
use serde::Serialize;

use crate::{
    config::PluginConfig,
    error::RivetError,
    types::{MetricValue, PluginDiagnostic},
};

#[cfg(not(feature = "plugins"))]
use super::manifest::PluginManifest;
#[cfg(feature = "plugins")]
use super::manifest::{AnalyzeOutputs, PluginManifest};

#[derive(Debug, Clone, Serialize)]
pub struct PluginAnalysisInput {
    pub source: String,
    pub function_name: String,
    pub language: String,
    pub sexp: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone)]
pub struct PluginHost {
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    config: PluginConfig,
    #[cfg_attr(not(feature = "plugins"), allow(dead_code))]
    plugins: Vec<LoadedPlugin>,
}

#[derive(Debug, Clone)]
#[cfg_attr(not(feature = "plugins"), allow(dead_code))]
struct LoadedPlugin {
    wasm_bytes: Vec<u8>,
    manifest: PluginManifest,
}

impl PluginHost {
    #[must_use]
    pub const fn new(config: PluginConfig) -> Self {
        Self {
            config,
            plugins: Vec::new(),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn register_plugin(&mut self, wasm_bytes: &[u8]) -> Result<(), RivetError> {
        #[cfg(not(feature = "plugins"))]
        {
            let _ = wasm_bytes;
            Err(RivetError::UnsupportedFeature(
                "WASM plugin loading requires the `plugins` feature".to_string(),
            ))
        }

        #[cfg(feature = "plugins")]
        {
            let manifest = load_manifest(wasm_bytes, &self.config)?;
            validate_manifest(&manifest)?;
            self.plugins.push(LoadedPlugin {
                wasm_bytes: wasm_bytes.to_vec(),
                manifest,
            });
            Ok(())
        }
    }

    #[must_use]
    pub fn analyze_function(
        &self,
        input: &PluginAnalysisInput,
    ) -> (
        std::collections::HashMap<String, MetricValue>,
        Vec<PluginDiagnostic>,
    ) {
        #[cfg(not(feature = "plugins"))]
        {
            let _ = input;
            (std::collections::HashMap::new(), Vec::new())
        }

        #[cfg(feature = "plugins")]
        {
            let mut metrics = std::collections::HashMap::new();
            let mut diagnostics = Vec::new();

            for loaded in &self.plugins {
                if !plugin_supports_language(&loaded.manifest, &input.language) {
                    continue;
                }

                match invoke_plugin(&loaded.wasm_bytes, &self.config, "analyze", input)
                    .and_then(|output| parse_outputs(&output))
                {
                    Ok(outputs) => {
                        for output in outputs {
                            if let Some(previous) =
                                metrics.insert(output.metric_id.clone(), output.value)
                            {
                                let _ = previous;
                                diagnostics.push(PluginDiagnostic {
                                    plugin_name: loaded.manifest.name.clone(),
                                    function_name: Some(input.function_name.clone()),
                                    metric_name: Some(output.metric_id),
                                    message: "duplicate metric id returned by plugin execution"
                                        .to_string(),
                                    severity: crate::types::Severity::Warning,
                                });
                            }
                        }
                    }
                    Err(error) => diagnostics.push(PluginDiagnostic {
                        plugin_name: loaded.manifest.name.clone(),
                        function_name: Some(input.function_name.clone()),
                        metric_name: None,
                        message: error.to_string(),
                        severity: crate::types::Severity::Warning,
                    }),
                }
            }

            (metrics, diagnostics)
        }
    }
}

#[cfg(feature = "plugins")]
fn plugin_supports_language(manifest: &PluginManifest, language: &str) -> bool {
    manifest.supported_languages.is_empty()
        || manifest
            .supported_languages
            .iter()
            .any(|item| item.eq_ignore_ascii_case(language))
}

#[cfg(feature = "plugins")]
fn load_manifest(wasm_bytes: &[u8], config: &PluginConfig) -> Result<PluginManifest, RivetError> {
    let manifest = invoke_manifest(wasm_bytes, config).and_then(|output| {
        serde_json::from_str::<PluginManifest>(&output).map_err(|error| to_plugin_error(&error))
    })?;
    Ok(manifest)
}

#[cfg(feature = "plugins")]
fn parse_outputs(output: &str) -> Result<Vec<super::manifest::AnalyzeOutput>, RivetError> {
    serde_json::from_str::<AnalyzeOutputs>(output)
        .map(AnalyzeOutputs::into_vec)
        .map_err(|error| to_plugin_error(&error))
}

#[cfg(feature = "plugins")]
fn validate_manifest(manifest: &PluginManifest) -> Result<(), RivetError> {
    if manifest.name.trim().is_empty() {
        return Err(RivetError::Plugin(
            "plugin manifest name cannot be empty".to_string(),
        ));
    }
    if manifest.metrics.is_empty() {
        return Err(RivetError::Plugin(
            "plugin manifest must declare at least one metric id".to_string(),
        ));
    }

    let mut ids = std::collections::HashSet::new();
    for metric in &manifest.metrics {
        if metric.trim().is_empty() || !ids.insert(metric) {
            return Err(RivetError::Plugin(format!(
                "plugin manifest contains an invalid or duplicate metric id: {metric}"
            )));
        }
    }

    Ok(())
}

#[cfg(feature = "plugins")]
fn invoke_plugin<T: Serialize>(
    wasm_bytes: &[u8],
    config: &PluginConfig,
    function_name: &str,
    input: T,
) -> Result<String, RivetError> {
    let manifest = Manifest::new([Wasm::data(wasm_bytes)])
        .with_memory_max(config.max_memory_pages)
        .with_timeout(Duration::from_millis(config.timeout_ms));
    let mut plugin = PluginBuilder::new(manifest)
        .with_wasi(false)
        .build()
        .map_err(|error| to_plugin_error(&error))?;
    let payload = serde_json::to_string(&input).map_err(|error| to_plugin_error(&error))?;
    plugin
        .call::<String, String>(function_name, payload)
        .map_err(|error| to_plugin_error(&error))
}

#[cfg(feature = "plugins")]
fn invoke_manifest(wasm_bytes: &[u8], config: &PluginConfig) -> Result<String, RivetError> {
    let manifest = Manifest::new([Wasm::data(wasm_bytes)])
        .with_memory_max(config.max_memory_pages)
        .with_timeout(Duration::from_millis(config.timeout_ms));
    let mut plugin = PluginBuilder::new(manifest)
        .with_wasi(false)
        .build()
        .map_err(|error| to_plugin_error(&error))?;
    plugin
        .call::<&str, String>("manifest", "")
        .map_err(|error| to_plugin_error(&error))
}

#[cfg(feature = "plugins")]
fn to_plugin_error(error: &impl ToString) -> RivetError {
    RivetError::Plugin(error.to_string())
}
