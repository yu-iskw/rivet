//! Core types for rivet-core.

use std::fmt;
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::RivetError;

/// A supported programming language.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum Language {
    /// Rust
    Rust,
    /// Python
    Python,
    /// TypeScript
    TypeScript,
    /// JavaScript
    JavaScript,
    /// Go
    Go,
    /// Java
    Java,
    /// C++
    Cpp,
    /// C
    C,
    /// C#
    CSharp,
    /// Ruby
    Ruby,
    /// PHP
    Php,
    /// Swift
    Swift,
    /// Kotlin
    Kotlin,
    /// Scala
    Scala,
    /// Lua
    Lua,
    /// R
    R,
    /// SQL
    Sql,
    /// HCL (Terraform)
    Hcl,
    /// YAML
    Yaml,
    /// TOML
    Toml,
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::TypeScript => "typescript",
            Self::JavaScript => "javascript",
            Self::Go => "go",
            Self::Java => "java",
            Self::Cpp => "cpp",
            Self::C => "c",
            Self::CSharp => "csharp",
            Self::Ruby => "ruby",
            Self::Php => "php",
            Self::Swift => "swift",
            Self::Kotlin => "kotlin",
            Self::Scala => "scala",
            Self::Lua => "lua",
            Self::R => "r",
            Self::Sql => "sql",
            Self::Hcl => "hcl",
            Self::Yaml => "yaml",
            Self::Toml => "toml",
        };
        write!(f, "{s}")
    }
}

impl FromStr for Language {
    type Err = RivetError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "rust" | "rs" => Ok(Self::Rust),
            "python" | "py" => Ok(Self::Python),
            "typescript" | "ts" => Ok(Self::TypeScript),
            "javascript" | "js" => Ok(Self::JavaScript),
            "go" => Ok(Self::Go),
            "java" => Ok(Self::Java),
            "cpp" | "c++" | "cxx" => Ok(Self::Cpp),
            "c" => Ok(Self::C),
            "csharp" | "c#" | "cs" => Ok(Self::CSharp),
            "ruby" | "rb" => Ok(Self::Ruby),
            "php" => Ok(Self::Php),
            "swift" => Ok(Self::Swift),
            "kotlin" | "kt" => Ok(Self::Kotlin),
            "scala" => Ok(Self::Scala),
            "lua" => Ok(Self::Lua),
            "r" => Ok(Self::R),
            "sql" => Ok(Self::Sql),
            "hcl" | "tf" => Ok(Self::Hcl),
            "yaml" | "yml" => Ok(Self::Yaml),
            "toml" => Ok(Self::Toml),
            _ => Err(RivetError::UnsupportedLanguage(s.to_owned())),
        }
    }
}

/// A source location (line/column range).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Location {
    /// Start line (1-based).
    pub start_line: u32,
    /// End line (1-based).
    pub end_line: u32,
    /// Start column (0-based).
    pub start_column: u32,
    /// End column (0-based).
    pub end_column: u32,
}

/// Input for analyzing a single file.
#[derive(Debug)]
pub struct FileInput {
    /// Source bytes.
    pub source: Vec<u8>,
    /// Programming language.
    pub language: Language,
    /// File path (for reporting).
    pub file_path: PathBuf,
}
