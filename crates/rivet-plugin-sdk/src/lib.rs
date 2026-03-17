use std::collections::HashMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeInput {
    pub source: String,
    pub function_name: String,
    pub language: String,
    pub sexp: String,
    pub start_line: u32,
    pub end_line: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyzeOutput {
    pub metric_id: String,
    pub display_name: String,
    pub value: MetricValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Float(f64),
    Composite(HashMap<String, Self>),
}
