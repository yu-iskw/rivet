#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::must_use_candidate
)]

use std::collections::HashSet;

use crate::{language::LanguageConfig, types::FunctionAnalysis};

pub fn compute_cyclomatic_complexity(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> u32 {
    let _ = (source, language);
    1 + control_flow_ranges(node).len() as u32
}

pub fn compute_cognitive_complexity(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> u32 {
    let _ = (source, language);
    let ranges = control_flow_ranges(node);
    let mut total = 0;
    for (index, current) in ranges.iter().enumerate() {
        let depth = ranges[..index]
            .iter()
            .filter(|candidate| candidate.0 <= current.0 && candidate.1 >= current.1)
            .count() as u32;
        total += 1 + depth.saturating_sub(1);
    }
    total
}

pub fn compute_nesting_depth(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> u32 {
    let _ = (source, language);
    let ranges = control_flow_ranges(node);
    ranges
        .iter()
        .enumerate()
        .map(|(index, current)| {
            ranges[..index]
                .iter()
                .filter(|candidate| candidate.0 <= current.0 && candidate.1 >= current.1)
                .count() as u32
        })
        .max()
        .unwrap_or(0)
}

fn control_flow_ranges(node: tree_sitter::Node<'_>) -> Vec<(usize, usize)> {
    let mut ranges = Vec::new();
    let mut seen = HashSet::new();
    collect_control_flow_ranges(node, &mut ranges, &mut seen);
    ranges.sort_unstable();
    ranges
}

fn collect_control_flow_ranges(
    node: tree_sitter::Node<'_>,
    ranges: &mut Vec<(usize, usize)>,
    seen: &mut HashSet<(usize, usize)>,
) {
    if is_control_flow_kind(node.kind()) {
        let value = (node.start_byte(), node.end_byte());
        if seen.insert(value) {
            ranges.push(value);
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_control_flow_ranges(child, ranges, seen);
    }
}

fn is_control_flow_kind(kind: &str) -> bool {
    matches!(
        kind,
        "if_expression"
            | "else_clause"
            | "for_expression"
            | "while_expression"
            | "loop_expression"
            | "match_arm"
            | "if_statement"
            | "elif_clause"
            | "for_statement"
            | "while_statement"
            | "except_clause"
            | "conditional_expression"
            | "catch_clause"
    )
}

#[must_use]
pub fn total_complexity(functions: &[FunctionAnalysis]) -> (f64, f64, f64) {
    if functions.is_empty() {
        return (0.0, 0.0, 0.0);
    }

    let total = functions
        .iter()
        .map(|function| f64::from(function.cyclomatic_complexity))
        .sum::<f64>();
    let average = total / functions.len() as f64;
    let maximum = functions
        .iter()
        .map(|function| f64::from(function.cyclomatic_complexity))
        .fold(0.0, f64::max);

    (total, average, maximum)
}
