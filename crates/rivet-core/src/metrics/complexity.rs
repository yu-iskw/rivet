#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::must_use_candidate
)]

use std::collections::HashSet;

use crate::{language::LanguageConfig, types::FunctionAnalysis};
use tree_sitter::{QueryCursor, StreamingIterator};

pub fn compute_cyclomatic_complexity(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> u32 {
    let events = complexity_events(node, source, language);
    1 + events
        .iter()
        .filter(|event| {
            matches!(
                event.kind,
                EventKind::CcOnly | EventKind::Structural | EventKind::Fundamental
            )
        })
        .count() as u32
}

pub fn compute_cognitive_complexity(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> u32 {
    let events = complexity_events(node, source, language);
    let nesting_ranges = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::Structural))
        .map(|event| event.range)
        .collect::<Vec<_>>();

    let mut total = 0;
    for event in &events {
        match event.kind {
            EventKind::Structural => {
                let depth = nesting_depth_for_range(event.range, &nesting_ranges);
                total += 1 + depth.saturating_sub(1);
            }
            EventKind::Fundamental => {
                total += 1;
            }
            EventKind::CcOnly => {}
        }
    }

    total
}

pub fn compute_nesting_depth(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> u32 {
    let events = complexity_events(node, source, language);
    let nesting_ranges = events
        .iter()
        .filter(|event| matches!(event.kind, EventKind::Structural))
        .map(|event| event.range)
        .collect::<Vec<_>>();

    nesting_ranges
        .iter()
        .map(|range| nesting_depth_for_range(*range, &nesting_ranges))
        .max()
        .unwrap_or(0)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct Event {
    range: (usize, usize),
    kind: EventKind,
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum EventKind {
    Structural,
    Fundamental,
    CcOnly,
}

fn complexity_events(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> Vec<Event> {
    let mut cursor = QueryCursor::new();
    let mut captures = cursor.captures(&language.control_flow_query, node, source);
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    let capture_names = language.control_flow_query.capture_names();

    captures.advance();
    while let Some((query_match, capture_index)) = captures.get() {
        let capture = query_match.captures[*capture_index];
        let Some(capture_name) = capture_names.get(capture.index as usize) else {
            captures.advance();
            continue;
        };
        let Some(kind) = classify_capture(capture_name) else {
            captures.advance();
            continue;
        };
        let event = Event {
            range: (capture.node.start_byte(), capture.node.end_byte()),
            kind,
        };
        if seen.insert(event) {
            events.push(event);
        }
        captures.advance();
    }

    events.sort_unstable_by_key(|event| event.range);
    events
}

fn classify_capture(capture_name: &str) -> Option<EventKind> {
    let capture_name = capture_name.replace('.', "_");
    match capture_name.as_str() {
        "cc_branch" => Some(EventKind::CcOnly),
        "cc_fundamental" | "cognitive_fundamental" => Some(EventKind::Fundamental),
        "cognitive_structural" => Some(EventKind::Structural),
        _ => None,
    }
}

fn nesting_depth_for_range(range: (usize, usize), nesting_ranges: &[(usize, usize)]) -> u32 {
    nesting_ranges
        .iter()
        .filter(|candidate| candidate.0 <= range.0 && candidate.1 >= range.1)
        .count() as u32
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
