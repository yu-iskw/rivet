#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::must_use_candidate,
    clippy::suboptimal_flops
)]

use std::collections::HashSet;

use tree_sitter::{QueryCursor, StreamingIterator};

use crate::{language::LanguageConfig, types::HalsteadMetrics};

pub fn compute_halstead(
    node: tree_sitter::Node<'_>,
    source: &[u8],
    language: &LanguageConfig,
) -> HalsteadMetrics {
    let operators = capture_texts(&language.operator_query, node, source);
    let operands = capture_texts(&language.operand_query, node, source);

    let n1 = operators.iter().collect::<HashSet<_>>().len() as u32;
    let n2 = operands.iter().collect::<HashSet<_>>().len() as u32;
    let big_n1 = operators.len() as u32;
    let big_n2 = operands.len() as u32;
    let vocabulary = n1 + n2;
    let length = big_n1 + big_n2;
    let calculated_length = if n1 == 0 || n2 == 0 {
        0.0
    } else {
        (n1 as f64) * (n1 as f64).log2() + (n2 as f64) * (n2 as f64).log2()
    };
    let volume = if vocabulary <= 1 {
        0.0
    } else {
        (length as f64) * (vocabulary as f64).log2()
    };
    let difficulty = if n2 == 0 {
        0.0
    } else {
        (n1 as f64 / 2.0) * (big_n2 as f64 / n2 as f64)
    };
    let effort = difficulty * volume;

    HalsteadMetrics {
        n1,
        n2,
        big_n1,
        big_n2,
        vocabulary,
        length,
        calculated_length,
        volume,
        difficulty,
        effort,
        time: effort / 18.0,
        bugs: volume / 3000.0,
    }
}

fn capture_texts(
    query: &tree_sitter::Query,
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Vec<String> {
    let mut cursor = QueryCursor::new();
    let mut values = Vec::new();

    let mut matches = cursor.matches(query, node, source);
    while let Some(query_match) = matches.get() {
        for capture in query_match.captures {
            if let Ok(text) = capture.node.utf8_text(source) {
                values.push(text.to_string());
            }
        }
        matches.advance();
    }

    values
}
