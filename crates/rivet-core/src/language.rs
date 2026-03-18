use std::{
    collections::HashMap,
    hash::{Hash, Hasher},
    path::Path,
    str::FromStr,
};

#[cfg(feature = "lang-all")]
use std::collections::HashSet;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageSupportLevel {
    Full,
    ParseOnly,
}

impl LanguageSupportLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::ParseOnly => "parse_only",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LanguageSource {
    BuiltIn,
    LanguagePack,
}

impl LanguageSource {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BuiltIn => "built_in",
            Self::LanguagePack => "language_pack",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageDescriptor {
    pub id: String,
    pub display_name: String,
    pub support_level: LanguageSupportLevel,
    pub source: LanguageSource,
    pub extensions: Vec<String>,
}

impl LanguageDescriptor {
    #[must_use]
    pub fn full(language: Language, extensions: &[&str]) -> Self {
        Self {
            id: language.as_str().to_string(),
            display_name: language_display_name(language).to_string(),
            support_level: LanguageSupportLevel::Full,
            source: LanguageSource::BuiltIn,
            extensions: extensions
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
        }
    }

    #[must_use]
    pub fn parse_only(
        id: &str,
        display_name: &str,
        extensions: &[&str],
        source: LanguageSource,
    ) -> Self {
        Self {
            id: id.to_ascii_lowercase(),
            display_name: display_name.to_string(),
            support_level: LanguageSupportLevel::ParseOnly,
            source,
            extensions: extensions
                .iter()
                .map(|extension| (*extension).to_string())
                .collect(),
        }
    }

    #[must_use]
    pub fn matches_extension(&self, extension: &str) -> bool {
        self.extensions
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(extension))
    }
}

trait LanguageProvider {
    fn contribute(&self, builder: &mut LanguageRegistryBuilder);
}

#[derive(Default)]
struct LanguageRegistryBuilder {
    languages: HashMap<Language, LanguageConfig>,
    descriptors: HashMap<String, LanguageDescriptor>,
}

impl LanguageRegistryBuilder {
    fn register_full(&mut self, registration: &LanguageRegistration) -> Result<(), RivetError> {
        let descriptor = LanguageDescriptor::full(registration.language, registration.extensions);
        self.languages
            .insert(registration.language, build_language_config(registration)?);
        self.descriptors.insert(descriptor.id.clone(), descriptor);
        Ok(())
    }

    fn finish(self) -> LanguageRegistry {
        let mut descriptors = self.descriptors.into_values().collect::<Vec<_>>();
        descriptors.sort_by(|left, right| left.id.cmp(&right.id));

        LanguageRegistry {
            languages: self.languages,
            descriptors,
        }
    }
}

struct BuiltInLanguageProvider;

impl LanguageProvider for BuiltInLanguageProvider {
    fn contribute(&self, builder: &mut LanguageRegistryBuilder) {
        for registration in registrations() {
            builder
                .register_full(&registration)
                .expect("built-in language registration should compile");
        }
    }
}

#[cfg(feature = "lang-all")]
struct LanguagePackProvider;

#[cfg(feature = "lang-all")]
impl LanguageProvider for LanguagePackProvider {
    fn contribute(&self, builder: &mut LanguageRegistryBuilder) {
        let mut seen = builder.descriptors.keys().cloned().collect::<HashSet<_>>();

        for language_id in LANGUAGE_PACK_LANGUAGE_IDS {
            let canonical = canonical_language_pack_id(language_id);
            if seen.contains(canonical) {
                continue;
            }

            let descriptor = LanguageDescriptor::parse_only(
                canonical,
                language_pack_display_name(canonical),
                language_pack_extensions(canonical),
                LanguageSource::LanguagePack,
            );
            seen.insert(descriptor.id.clone());
            builder
                .descriptors
                .insert(descriptor.id.clone(), descriptor);
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

struct LanguageRegistration {
    language: Language,
    grammar: tree_sitter::Language,
    query_dir: &'static str,
    comment_prefixes: &'static [&'static str],
    extensions: &'static [&'static str],
}

pub struct LanguageRegistry {
    languages: HashMap<Language, LanguageConfig>,
    descriptors: Vec<LanguageDescriptor>,
}

impl LanguageRegistry {
    pub fn new() -> Result<Self, RivetError> {
        let mut builder = LanguageRegistryBuilder::default();
        BuiltInLanguageProvider.contribute(&mut builder);
        #[cfg(feature = "lang-all")]
        LanguagePackProvider.contribute(&mut builder);
        Ok(builder.finish())
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

    #[must_use]
    pub fn available_languages(&self) -> Vec<LanguageDescriptor> {
        self.descriptors.clone()
    }
}

#[must_use]
pub fn analysis_fingerprint() -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    env!("CARGO_PKG_VERSION").hash(&mut hasher);

    for registration in registrations() {
        registration.language.as_str().hash(&mut hasher);
        registration.query_dir.hash(&mut hasher);
        registration.comment_prefixes.hash(&mut hasher);
        registration.extensions.hash(&mut hasher);
        function_query_source(registration.query_dir).hash(&mut hasher);
        control_flow_query_source(registration.query_dir).hash(&mut hasher);
        operator_query_source(registration.query_dir).hash(&mut hasher);
        operand_query_source(registration.query_dir).hash(&mut hasher);
    }

    #[cfg(feature = "lang-all")]
    for language_id in LANGUAGE_PACK_LANGUAGE_IDS {
        let canonical = canonical_language_pack_id(language_id);
        canonical.hash(&mut hasher);
        language_pack_display_name(canonical).hash(&mut hasher);
        language_pack_extensions(canonical).hash(&mut hasher);
    }

    format!("{:016x}", hasher.finish())
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

fn build_language_config(
    registration: &LanguageRegistration,
) -> Result<LanguageConfig, RivetError> {
    Ok(LanguageConfig {
        function_query: compile_query(
            &registration.grammar,
            function_query_source(registration.query_dir),
            registration.language.as_str(),
        )?,
        control_flow_query: compile_query(
            &registration.grammar,
            control_flow_query_source(registration.query_dir),
            registration.language.as_str(),
        )?,
        operator_query: compile_query(
            &registration.grammar,
            operator_query_source(registration.query_dir),
            registration.language.as_str(),
        )?,
        operand_query: compile_query(
            &registration.grammar,
            operand_query_source(registration.query_dir),
            registration.language.as_str(),
        )?,
        grammar: registration.grammar.clone(),
        comment_prefixes: registration.comment_prefixes.to_vec(),
    })
}

#[allow(clippy::match_same_arms)]
fn function_query_source(query_dir: &str) -> &'static str {
    match query_dir {
        "c" => include_str!("../../../queries/c/functions.scm"),
        "cpp" => include_str!("../../../queries/cpp/functions.scm"),
        "csharp" => include_str!("../../../queries/csharp/functions.scm"),
        "go" => include_str!("../../../queries/go/functions.scm"),
        "java" => include_str!("../../../queries/java/functions.scm"),
        "javascript" => include_str!("../../../queries/javascript/functions.scm"),
        "kotlin" => include_str!("../../../queries/kotlin/functions.scm"),
        "php" => include_str!("../../../queries/php/functions.scm"),
        "python" => include_str!("../../../queries/python/functions.scm"),
        "ruby" => include_str!("../../../queries/ruby/functions.scm"),
        "rust" => include_str!("../../../queries/rust/functions.scm"),
        "typescript" => include_str!("../../../queries/typescript/functions.scm"),
        _ => unreachable!("unknown query directory"),
    }
}

#[allow(clippy::match_same_arms)]
fn control_flow_query_source(query_dir: &str) -> &'static str {
    match query_dir {
        "c" => include_str!("../../../queries/c/control_flow.scm"),
        "cpp" => include_str!("../../../queries/cpp/control_flow.scm"),
        "csharp" => include_str!("../../../queries/csharp/control_flow.scm"),
        "go" => include_str!("../../../queries/go/control_flow.scm"),
        "java" => include_str!("../../../queries/java/control_flow.scm"),
        "javascript" => include_str!("../../../queries/javascript/control_flow.scm"),
        "kotlin" => include_str!("../../../queries/kotlin/control_flow.scm"),
        "php" => include_str!("../../../queries/php/control_flow.scm"),
        "python" => include_str!("../../../queries/python/control_flow.scm"),
        "ruby" => include_str!("../../../queries/ruby/control_flow.scm"),
        "rust" => include_str!("../../../queries/rust/control_flow.scm"),
        "typescript" => include_str!("../../../queries/typescript/control_flow.scm"),
        _ => unreachable!("unknown query directory"),
    }
}

#[allow(clippy::match_same_arms)]
fn operator_query_source(query_dir: &str) -> &'static str {
    match query_dir {
        "c" => include_str!("../../../queries/c/operators.scm"),
        "cpp" => include_str!("../../../queries/cpp/operators.scm"),
        "csharp" => include_str!("../../../queries/csharp/operators.scm"),
        "go" => include_str!("../../../queries/go/operators.scm"),
        "java" => include_str!("../../../queries/java/operators.scm"),
        "javascript" => include_str!("../../../queries/javascript/operators.scm"),
        "kotlin" => include_str!("../../../queries/kotlin/operators.scm"),
        "php" => include_str!("../../../queries/php/operators.scm"),
        "python" => include_str!("../../../queries/python/operators.scm"),
        "ruby" => include_str!("../../../queries/ruby/operators.scm"),
        "rust" => include_str!("../../../queries/rust/operators.scm"),
        "typescript" => include_str!("../../../queries/typescript/operators.scm"),
        _ => unreachable!("unknown query directory"),
    }
}

#[allow(clippy::match_same_arms)]
fn operand_query_source(query_dir: &str) -> &'static str {
    match query_dir {
        "c" => include_str!("../../../queries/c/operands.scm"),
        "cpp" => include_str!("../../../queries/cpp/operands.scm"),
        "csharp" => include_str!("../../../queries/csharp/operands.scm"),
        "go" => include_str!("../../../queries/go/operands.scm"),
        "java" => include_str!("../../../queries/java/operands.scm"),
        "javascript" => include_str!("../../../queries/javascript/operands.scm"),
        "kotlin" => include_str!("../../../queries/kotlin/operands.scm"),
        "php" => include_str!("../../../queries/php/operands.scm"),
        "python" => include_str!("../../../queries/python/operands.scm"),
        "ruby" => include_str!("../../../queries/ruby/operands.scm"),
        "rust" => include_str!("../../../queries/rust/operands.scm"),
        "typescript" => include_str!("../../../queries/typescript/operands.scm"),
        _ => unreachable!("unknown query directory"),
    }
}

const fn language_display_name(language: Language) -> &'static str {
    match language {
        Language::Rust => "Rust",
        Language::Python => "Python",
        Language::TypeScript => "TypeScript",
        Language::JavaScript => "JavaScript",
        Language::Go => "Go",
        Language::Java => "Java",
        Language::C => "C",
        Language::Cpp => "C++",
        Language::CSharp => "C#",
        Language::Ruby => "Ruby",
        Language::Php => "PHP",
        Language::Kotlin => "Kotlin",
    }
}

#[cfg(feature = "lang-all")]
fn canonical_language_pack_id(language_id: &str) -> &str {
    match language_id {
        "bazel" => "starlark",
        "gradle" => "groovy",
        "ignorefile" => "gitignore",
        "lisp" => "commonlisp",
        "makefile" => "make",
        "shell" => "bash",
        other => other,
    }
}

#[cfg(feature = "lang-all")]
const LANGUAGE_PACK_LANGUAGE_IDS: &[&str] = &[
    "actionscript",
    "ada",
    "agda",
    "apex",
    "arduino",
    "asm",
    "astro",
    "bash",
    "beancount",
    "bibtex",
    "bicep",
    "bitbake",
    "bsl",
    "c",
    "cairo",
    "capnp",
    "chatito",
    "clarity",
    "clojure",
    "cmake",
    "cobol",
    "comment",
    "commonlisp",
    "cpon",
    "cpp",
    "css",
    "csv",
    "cuda",
    "d",
    "dart",
    "dockerfile",
    "doxygen",
    "dtd",
    "elisp",
    "elixir",
    "elm",
    "erlang",
    "fennel",
    "firrtl",
    "fish",
    "fortran",
    "fsharp",
    "fsharp_signature",
    "func",
    "gdscript",
    "gitattributes",
    "gitcommit",
    "gitignore",
    "gleam",
    "glsl",
    "gn",
    "go",
    "gomod",
    "gosum",
    "graphql",
    "groovy",
    "gstlaunch",
    "hack",
    "hare",
    "haskell",
    "haxe",
    "hcl",
    "heex",
    "hlsl",
    "html",
    "hyprlang",
    "ini",
    "ispc",
    "janet",
    "java",
    "javascript",
    "jsdoc",
    "json",
    "jsonnet",
    "julia",
    "kconfig",
    "kdl",
    "kotlin",
    "latex",
    "linkerscript",
    "llvm",
    "lua",
    "luadoc",
    "luap",
    "luau",
    "magik",
    "make",
    "markdown",
    "markdown_inline",
    "matlab",
    "mermaid",
    "meson",
    "netlinx",
    "nim",
    "ninja",
    "nix",
    "nqc",
    "objc",
    "ocaml",
    "ocaml_interface",
    "odin",
    "org",
    "pascal",
    "pem",
    "perl",
    "pgn",
    "php",
    "po",
    "pony",
    "powershell",
    "printf",
    "prisma",
    "properties",
    "proto",
    "psv",
    "puppet",
    "purescript",
    "pymanifest",
    "python",
    "qmldir",
    "qmljs",
    "query",
    "r",
    "racket",
    "re2c",
    "readline",
    "rego",
    "requirements",
    "ron",
    "rst",
    "ruby",
    "rust",
    "scala",
    "scheme",
    "scss",
    "smali",
    "smithy",
    "solidity",
    "sparql",
    "sql",
    "squirrel",
    "starlark",
    "svelte",
    "swift",
    "tablegen",
    "tcl",
    "terraform",
    "test",
    "thrift",
    "toml",
    "tsv",
    "tsx",
    "twig",
    "typescript",
    "typst",
    "udev",
    "ungrammar",
    "uxntal",
    "v",
    "verilog",
    "vhdl",
    "vim",
    "vue",
    "wast",
    "wat",
    "wgsl",
    "xcompose",
    "xml",
    "yuck",
    "zig",
];

#[cfg(feature = "lang-all")]
fn language_pack_display_name(language_id: &str) -> &'static str {
    match language_id {
        "bash" => "Bash",
        "commonlisp" => "Common Lisp",
        "cmake" => "CMake",
        "cpp" => "C++",
        "csharp" => "C#",
        "css" => "CSS",
        "csv" => "CSV",
        "dockerfile" => "Dockerfile",
        "fsharp" => "F#",
        "graphql" => "GraphQL",
        "hcl" => "HCL",
        "heex" => "HEEx",
        "html" => "HTML",
        "ini" => "INI",
        "json" => "JSON",
        "jsonnet" => "Jsonnet",
        "jsx" => "JSX",
        "kdl" => "KDL",
        "latex" => "LaTeX",
        "lua" => "Lua",
        "luau" => "Luau",
        "make" => "Make",
        "markdown" => "Markdown",
        "objc" => "Objective-C",
        "ocaml" => "OCaml",
        "org" => "Org",
        "php" => "PHP",
        "powershell" => "PowerShell",
        "proto" => "Protocol Buffers",
        "psv" => "PSV",
        "qmljs" => "QML/JS",
        "ron" => "RON",
        "rst" => "reStructuredText",
        "scala" => "Scala",
        "scss" => "SCSS",
        "sql" => "SQL",
        "starlark" => "Starlark",
        "svg" => "SVG",
        "toml" => "TOML",
        "tsx" => "TSX",
        "tsv" => "TSV",
        "typescript" => "TypeScript",
        "vim" => "Vim Script",
        "vue" => "Vue",
        "xml" => "XML",
        "yaml" => "YAML",
        "zig" => "Zig",
        _ => Box::leak(
            language_id
                .split(['-', '_'])
                .map(|segment| {
                    let mut chars = segment.chars();
                    let Some(first) = chars.next() else {
                        return String::new();
                    };
                    format!(
                        "{}{}",
                        first.to_ascii_uppercase(),
                        chars.as_str().to_ascii_lowercase()
                    )
                })
                .collect::<Vec<_>>()
                .join(" ")
                .into_boxed_str(),
        ),
    }
}

#[cfg(feature = "lang-all")]
fn language_pack_extensions(language_id: &str) -> &'static [&'static str] {
    match language_id {
        "bash" => &["sh", "bash", "zsh"],
        "clojure" => &["clj", "cljs", "cljc"],
        "css" => &["css"],
        "dart" => &["dart"],
        "dockerfile" => &["dockerfile"],
        "elixir" => &["ex", "exs"],
        "erlang" => &["erl", "hrl"],
        "groovy" => &["groovy", "gradle"],
        "haskell" => &["hs"],
        "hcl" => &["hcl", "tf", "tfvars"],
        "html" => &["html", "htm"],
        "json" => &["json"],
        "jsonnet" => &["jsonnet"],
        "lua" => &["lua"],
        "markdown" => &["md", "markdown"],
        "nix" => &["nix"],
        "objc" => &["m", "mm"],
        "ocaml" => &["ml", "mli"],
        "perl" => &["pl", "pm", "t"],
        "proto" => &["proto"],
        "r" => &["r"],
        "scala" => &["scala"],
        "sql" => &["sql"],
        "swift" => &["swift"],
        "toml" => &["toml"],
        "tsx" => &["tsx"],
        "xml" => &["xml"],
        "yaml" => &["yml", "yaml"],
        "zig" => &["zig"],
        _ => &[],
    }
}

#[allow(clippy::vec_init_then_push)]
fn registrations() -> Vec<LanguageRegistration> {
    let mut registrations = Vec::new();

    #[cfg(feature = "lang-c")]
    registrations.push(LanguageRegistration {
        language: Language::C,
        grammar: tree_sitter_c::LANGUAGE.into(),
        query_dir: "c",
        comment_prefixes: &["//"],
        extensions: &["c"],
    });

    #[cfg(feature = "lang-cpp")]
    registrations.push(LanguageRegistration {
        language: Language::Cpp,
        grammar: tree_sitter_cpp::LANGUAGE.into(),
        query_dir: "cpp",
        comment_prefixes: &["//"],
        extensions: &["cpp", "cc", "cxx"],
    });

    #[cfg(feature = "lang-csharp")]
    registrations.push(LanguageRegistration {
        language: Language::CSharp,
        grammar: tree_sitter_c_sharp::LANGUAGE.into(),
        query_dir: "csharp",
        comment_prefixes: &["//"],
        extensions: &["cs"],
    });

    #[cfg(feature = "lang-go")]
    registrations.push(LanguageRegistration {
        language: Language::Go,
        grammar: tree_sitter_go::LANGUAGE.into(),
        query_dir: "go",
        comment_prefixes: &["//"],
        extensions: &["go"],
    });

    #[cfg(feature = "lang-java")]
    registrations.push(LanguageRegistration {
        language: Language::Java,
        grammar: tree_sitter_java::LANGUAGE.into(),
        query_dir: "java",
        comment_prefixes: &["//"],
        extensions: &["java"],
    });

    #[cfg(feature = "lang-javascript")]
    registrations.push(LanguageRegistration {
        language: Language::JavaScript,
        grammar: tree_sitter_javascript::LANGUAGE.into(),
        query_dir: "javascript",
        comment_prefixes: &["//"],
        extensions: &["js"],
    });

    #[cfg(feature = "lang-kotlin")]
    registrations.push(LanguageRegistration {
        language: Language::Kotlin,
        grammar: tree_sitter_kotlin_ng::LANGUAGE.into(),
        query_dir: "kotlin",
        comment_prefixes: &["//"],
        extensions: &["kt"],
    });

    #[cfg(feature = "lang-php")]
    registrations.push(LanguageRegistration {
        language: Language::Php,
        grammar: tree_sitter_php::LANGUAGE_PHP.into(),
        query_dir: "php",
        comment_prefixes: &["//", "#"],
        extensions: &["php"],
    });

    #[cfg(feature = "lang-python")]
    registrations.push(LanguageRegistration {
        language: Language::Python,
        grammar: tree_sitter_python::LANGUAGE.into(),
        query_dir: "python",
        comment_prefixes: &["#"],
        extensions: &["py"],
    });

    #[cfg(feature = "lang-ruby")]
    registrations.push(LanguageRegistration {
        language: Language::Ruby,
        grammar: tree_sitter_ruby::LANGUAGE.into(),
        query_dir: "ruby",
        comment_prefixes: &["#"],
        extensions: &["rb"],
    });

    #[cfg(feature = "lang-rust")]
    registrations.push(LanguageRegistration {
        language: Language::Rust,
        grammar: tree_sitter_rust::LANGUAGE.into(),
        query_dir: "rust",
        comment_prefixes: &["//"],
        extensions: &["rs"],
    });

    #[cfg(feature = "lang-typescript")]
    registrations.push(LanguageRegistration {
        language: Language::TypeScript,
        grammar: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        query_dir: "typescript",
        comment_prefixes: &["//"],
        extensions: &["ts"],
    });

    registrations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_exposes_full_support_descriptors_for_the_popular_slice() {
        let registry = LanguageRegistry::new().expect("registry");
        let supported = registry.supported_languages();
        let available = registry.available_languages();

        let supported_ids = supported
            .iter()
            .map(|language| language.as_str())
            .collect::<Vec<_>>();
        let available_ids = available
            .iter()
            .map(|descriptor| descriptor.id.as_str())
            .collect::<Vec<_>>();

        for supported_id in supported_ids {
            assert!(available_ids.contains(&supported_id));
        }
        assert!(available.iter().any(|descriptor| descriptor.id == "rust"));
        assert!(available.iter().any(|descriptor| {
            descriptor.support_level == LanguageSupportLevel::Full
                && descriptor.source == LanguageSource::BuiltIn
        }));
        #[cfg(feature = "lang-all")]
        assert!(available.iter().any(|descriptor| {
            descriptor.support_level == LanguageSupportLevel::ParseOnly
                && descriptor.source == LanguageSource::LanguagePack
        }));
    }

    #[test]
    fn rust_descriptor_contains_expected_metadata() {
        let registry = LanguageRegistry::new().expect("registry");
        let rust = registry
            .available_languages()
            .into_iter()
            .find(|descriptor| descriptor.id == "rust")
            .expect("rust descriptor");

        assert_eq!(rust.display_name, "Rust");
        assert_eq!(rust.support_level, LanguageSupportLevel::Full);
        assert_eq!(rust.source, LanguageSource::BuiltIn);
        assert_eq!(rust.extensions, vec!["rs"]);
    }
}
