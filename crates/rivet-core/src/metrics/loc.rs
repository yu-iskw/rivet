#![allow(clippy::cast_possible_truncation, clippy::must_use_candidate)]

use crate::types::{FileMetrics, HalsteadMetrics};

pub fn compute_file_metrics(source: &[u8], comment_prefixes: &[&str]) -> FileMetrics {
    let text = String::from_utf8_lossy(source);
    let lines = text.lines().collect::<Vec<_>>();
    let ploc = lines.len() as u32;
    let blank = lines.iter().filter(|line| line.trim().is_empty()).count() as u32;
    let cloc = lines
        .iter()
        .filter(|line| {
            let trimmed = line.trim();
            comment_prefixes
                .iter()
                .any(|prefix| !trimmed.is_empty() && trimmed.starts_with(prefix))
        })
        .count() as u32;
    let sloc = ploc.saturating_sub(blank + cloc);

    FileMetrics {
        nloc: sloc,
        sloc,
        ploc,
        lloc: sloc,
        cloc,
        blank,
        total_complexity: 0.0,
        avg_complexity: 0.0,
        max_complexity: 0.0,
        maintainability_index: 0.0,
        halstead: HalsteadMetrics::default(),
    }
}

pub fn compute_function_nloc(source: &[u8], start_line: u32, end_line: u32) -> u32 {
    let text = String::from_utf8_lossy(source);
    text.lines()
        .enumerate()
        .filter(|(index, line)| {
            let line_number = *index as u32 + 1;
            line_number >= start_line && line_number <= end_line && !line.trim().is_empty()
        })
        .count() as u32
}
