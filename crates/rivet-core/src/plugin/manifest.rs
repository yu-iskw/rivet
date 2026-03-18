use serde::{Deserialize, Serialize};

use crate::types::MetricValue;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub metrics: Vec<String>,
    #[serde(default)]
    pub supported_languages: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeOutput {
    pub metric_id: String,
    pub display_name: String,
    pub value: MetricValue,
}

#[cfg(feature = "plugins")]
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AnalyzeOutputs {
    Single(AnalyzeOutput),
    Multiple(Vec<AnalyzeOutput>),
}

#[cfg(feature = "plugins")]
impl AnalyzeOutputs {
    #[must_use]
    pub fn into_vec(self) -> Vec<AnalyzeOutput> {
        match self {
            Self::Single(output) => vec![output],
            Self::Multiple(outputs) => outputs,
        }
    }
}
