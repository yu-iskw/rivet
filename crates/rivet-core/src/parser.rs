#![allow(clippy::cast_possible_truncation)]

use crate::{error::RivetError, language::LanguageConfig, types::ParseError};

pub struct Parser {
    inner: tree_sitter::Parser,
}

pub struct ParseResult {
    pub tree: tree_sitter::Tree,
    pub errors: Vec<ParseError>,
}

impl Parser {
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: tree_sitter::Parser::new(),
        }
    }

    pub fn parse(
        &mut self,
        source: &[u8],
        language: &LanguageConfig,
    ) -> Result<ParseResult, RivetError> {
        self.inner
            .set_language(&language.grammar)
            .map_err(|error| RivetError::Parse(error.to_string()))?;

        let tree = self
            .inner
            .parse(source, None)
            .ok_or_else(|| RivetError::Parse("tree-sitter returned no tree".to_string()))?;

        let mut errors = Vec::new();
        collect_parse_errors(tree.root_node(), &mut errors);

        Ok(ParseResult { tree, errors })
    }
}

impl Default for Parser {
    fn default() -> Self {
        Self::new()
    }
}

fn collect_parse_errors(node: tree_sitter::Node<'_>, errors: &mut Vec<ParseError>) {
    if node.is_error() || node.is_missing() {
        let start = node.start_position();
        let end = node.end_position();
        errors.push(ParseError {
            start_line: start.row as u32 + 1,
            start_column: start.column as u32,
            end_line: end.row as u32 + 1,
            end_column: end.column as u32,
            message: format!("parse issue at {}", node.kind()),
        });
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_parse_errors(child, errors);
    }
}
