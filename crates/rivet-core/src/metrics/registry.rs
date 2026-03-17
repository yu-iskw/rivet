use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::RivetError;

pub trait MetricAnalyzer: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn analyze_function(
        &self,
        node: tree_sitter::Node<'_>,
        source: &[u8],
        language: &crate::language::LanguageConfig,
    ) -> Result<MetricValue, RivetError>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Float(f64),
    Composite(HashMap<String, Self>),
}

#[derive(Default)]
pub struct MetricRegistry;

impl MetricRegistry {
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self
    }
}
