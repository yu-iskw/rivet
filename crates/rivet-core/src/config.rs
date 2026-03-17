use serde::{Deserialize, Serialize};

use crate::types::Thresholds;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AnalyzerConfig {
    pub thresholds: Thresholds,
    pub jobs: Option<usize>,
}
