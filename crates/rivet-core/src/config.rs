use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::types::Thresholds;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyzerConfig {
    pub thresholds: Thresholds,
    pub threshold_overrides: Vec<PathThresholdOverride>,
    pub plugins: PluginConfig,
    pub jobs: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathThresholdOverride {
    pub paths: Vec<String>,
    pub thresholds: Thresholds,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub enabled: bool,
    pub discovery_paths: Vec<PathBuf>,
    pub entries: Vec<PluginEntryConfig>,
    pub max_memory_pages: u32,
    pub timeout_ms: u64,
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            discovery_paths: Vec::new(),
            entries: Vec::new(),
            max_memory_pages: 256,
            timeout_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntryConfig {
    pub path: PathBuf,
    pub name: Option<String>,
    pub enabled: bool,
}
