use std::{collections::HashMap, path::Path, str::FromStr};

use serde::{Deserialize, Serialize};
use tree_sitter::Query;

use crate::error::RivetError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    C,
    Cpp,
    CSharp,
    Ruby,
    Php,
    Kotlin,
}

impl Language {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Go => "go",
            Self::Java => "java",
            Self::C => "c",
            Self::Cpp => "cpp",
            Self::CSharp => "csharp",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Kotlin => "kotlin",
        }
    }

    pub fn from_path(path: &Path) -> Result<Self, RivetError> {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .ok_or_else(|| RivetError::UnsupportedLanguage(path.display().to_string()))?;
        Self::from_str(extension)
    }
}

impl FromStr for Language {
    type Err = RivetError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "rust" | "rs" => Ok(Self::Rust),
            "python" | "py" => Ok(Self::Python),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "javascript" | "js" => Ok(Self::JavaScript),
            "go" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "c" => Ok(Self::C),
            "cpp" | "c++" | "cc" | "cxx" => Ok(Self::Cpp),
            "csharp" | "c#" | "cs" => Ok(Self::CSharp),
            "ruby" | "rb" => Ok(Self::Ruby),
            "php" => Ok(Self::Php),
            "kotlin" | "kt" => Ok(Self::Kotlin),
            other => Err(RivetError::UnsupportedLanguage(other.to_string())),
        }
    }
}

pub struct LanguageConfig {
    pub grammar: tree_sitter::Language,
    pub function_query: Query,
    pub control_flow_query: Query,
    pub operator_query: Query,
    pub operand_query: Query,
    pub comment_prefixes: Vec<&'static str>,
}

pub struct LanguageRegistry {
    languages: HashMap<Language, LanguageConfig>,
}

impl LanguageRegistry {
    pub fn new() -> Result<Self, RivetError> {
        let mut languages = HashMap::new();

        #[cfg(feature = "lang-rust")]
        languages.insert(Language::Rust, rust_config()?);

        #[cfg(feature = "lang-python")]
        languages.insert(Language::Python, python_config()?);

        Ok(Self { languages })
    }

    pub fn get(&self, language: Language) -> Result<&LanguageConfig, RivetError> {
        self.languages
            .get(&language)
            .ok_or_else(|| RivetError::UnsupportedLanguage(language.as_str().to_string()))
    }

    #[must_use]
    pub fn supported_languages(&self) -> Vec<Language> {
        let mut values = self.languages.keys().copied().collect::<Vec<_>>();
        values.sort_by_key(|language| language.as_str());
        values
    }
}

fn compile_query(
    language: &tree_sitter::Language,
    source: &str,
    name: &str,
) -> Result<Query, RivetError> {
    Query::new(language, source).map_err(|error| RivetError::QueryCompilation {
        language: name.to_string(),
        message: error.to_string(),
    })
}

#[cfg(feature = "lang-rust")]
fn rust_config() -> Result<LanguageConfig, RivetError> {
    let grammar: tree_sitter::Language = tree_sitter_rust::LANGUAGE.into();
    Ok(LanguageConfig {
        function_query: compile_query(
            &grammar,
            include_str!("../../../queries/rust/functions.scm"),
            "rust",
        )?,
        control_flow_query: compile_query(
            &grammar,
            include_str!("../../../queries/rust/control_flow.scm"),
            "rust",
        )?,
        operator_query: compile_query(
            &grammar,
            include_str!("../../../queries/rust/operators.scm"),
            "rust",
        )?,
        operand_query: compile_query(
            &grammar,
            include_str!("../../../queries/rust/operands.scm"),
            "rust",
        )?,
        grammar,
        comment_prefixes: vec!["//"],
    })
}

#[cfg(feature = "lang-python")]
fn python_config() -> Result<LanguageConfig, RivetError> {
    let grammar: tree_sitter::Language = tree_sitter_python::LANGUAGE.into();
    Ok(LanguageConfig {
        function_query: compile_query(
            &grammar,
            include_str!("../../../queries/python/functions.scm"),
            "python",
        )?,
        control_flow_query: compile_query(
            &grammar,
            include_str!("../../../queries/python/control_flow.scm"),
            "python",
        )?,
        operator_query: compile_query(
            &grammar,
            include_str!("../../../queries/python/operators.scm"),
            "python",
        )?,
        operand_query: compile_query(
            &grammar,
            include_str!("../../../queries/python/operands.scm"),
            "python",
        )?,
        grammar,
        comment_prefixes: vec!["#"],
    })
}
