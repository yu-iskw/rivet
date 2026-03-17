# System Design Document: Rivet

## AI-Agent-Native Code Complexity Analyzer

**Version:** 1.0.0-draft
**Date:** 2026-03-17
**Status:** Design Phase

---

## Table of Contents

1. [Executive Summary](#1-executive-summary)
2. [Goals and Non-Goals](#2-goals-and-non-goals)
3. [Architecture Overview](#3-architecture-overview)
4. [Core Components](#4-core-components)
5. [Type System and Data Models](#5-type-system-and-data-models)
6. [Parser Layer](#6-parser-layer)
7. [Metrics Engine](#7-metrics-engine)
8. [Plugin System](#8-plugin-system)
9. [Language Bindings](#9-language-bindings)
10. [AI Agent Integration](#10-ai-agent-integration)
11. [LSP Server](#11-lsp-server)
12. [CLI Design](#12-cli-design)
13. [Output Formats](#13-output-formats)
14. [Configuration System](#14-configuration-system)
15. [Testing Strategy](#15-testing-strategy)
16. [Performance Engineering](#16-performance-engineering)
17. [Security Model](#17-security-model)
18. [Build System and CI/CD](#18-build-system-and-cicd)
19. [Agent-Driven Development Strategy](#19-agent-driven-development-strategy)
20. [Phased Implementation Plan](#20-phased-implementation-plan)
21. [Open Questions and Future Work](#21-open-questions-and-future-work)

---

## 1. Executive Summary

**Rivet** is a high-performance, governance-grade code complexity analyzer written in Rust. It is designed from the ground up to serve as foundational infrastructure for AI-agent-driven development workflows while remaining fully usable as a standalone CLI tool and library.

The tool computes software complexity metrics — cyclomatic complexity, cognitive complexity, Halstead metrics, lines of code, maintainability index, and more — across 170+ programming languages. It differentiates itself from existing tools (Lizard, rust-code-analysis, SonarQube) through five pillars:

1. **tree-sitter-powered parsing** for robust, incremental, error-tolerant analysis
2. **WASM-sandboxed plugin system** (via Extism) for safe, language-agnostic extensibility
3. **Native cross-language bindings** (PyO3 for Python, napi-rs for Node.js/TypeScript)
4. **AI-agent-first output** including SARIF v2.1.0, structured JSON, and an MCP server
5. **Real-time IDE integration** via an LSP server that surfaces complexity diagnostics, code lenses, and hover information directly in any editor

The system is implemented as a Cargo workspace monorepo with clean separation between the pure-Rust core library, the CLI, the MCP server, the LSP server, and the language binding crates.

---

## 2. Goals and Non-Goals

### Goals

| ID  | Goal                                                          | Rationale                                                                                   |
| :-- | :------------------------------------------------------------ | :------------------------------------------------------------------------------------------ |
| G1  | Compute CC, Cognitive, Halstead, LOC, MI metrics accurately   | Parity with established tools (Lizard, rust-code-analysis)                                  |
| G2  | Support 170+ languages via tree-sitter                        | Broad applicability without per-language maintenance burden                                 |
| G3  | Expose Python and TypeScript bindings with type stubs         | Enable integration into data pipelines, CI scripts, and agent frameworks                    |
| G4  | Provide WASM-sandboxed plugin system                          | Safe community extensibility; custom metrics without forking                                |
| G5  | Produce SARIF v2.1.0 output                                   | GitHub Code Scanning, Azure DevOps, IDE integration                                         |
| G6  | Run as an MCP server                                          | Direct integration with Claude Code, Cursor, Gemini CLI, Codex                              |
| G7  | Run as an LSP server with diagnostics, code lenses, and hover | Real-time complexity feedback in VS Code, Neovim, Zed, Helix, and any LSP-compatible editor |
| G8  | Support incremental analysis                                  | Performance at monorepo scale (100k+ files); essential for LSP responsiveness               |
| G9  | Be fully testable via snapshot tests                          | AI agents can validate changes via `cargo test` / `cargo insta review`                      |
| G10 | Target sub-100ms per-file analysis                            | Interactive feedback in editor/agent loops                                                  |
| G11 | Provide threshold-based pass/fail gates                       | CI/CD quality enforcement; make non-compliant code structurally impossible to merge         |

### Non-Goals

| ID  | Non-Goal                                           | Reason                                                        |
| :-- | :------------------------------------------------- | :------------------------------------------------------------ |
| NG1 | Full semantic analysis (type inference, data flow) | Requires language-specific compilers; out of scope            |
| NG2 | Exact Lizard output compatibility                  | We target correctness over backward compatibility             |
| NG3 | Code formatting or auto-fix                        | Separate concern; interoperate with formatters via SARIF      |
| NG4 | GUI or web dashboard                               | CLI + MCP + LSP + bindings provide sufficient interfaces      |
| NG5 | Windows-first development                          | Linux/macOS primary targets; Windows via CI cross-compilation |
| NG6 | Full DAP (Debug Adapter Protocol) integration      | Complexity analysis does not require debugger hooks           |

---

## 3. Architecture Overview

### 3.1 High-Level Architecture

```text
┌───────────────────────────────────────────────────────────────────────┐
│                           Consumer Layer                              │
│  ┌─────────┐ ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌────────────┐  │
│  │   CLI   │ │  MCP    │ │   LSP    │ │  Python  │ │  Node.js   │  │
│  │(clap v4)│ │ Server  │ │  Server  │ │  (PyO3)  │ │ (napi-rs)  │  │
│  │         │ │ (rmcp)  │ │(tower-   │ │(maturin) │ │            │  │
│  │         │ │         │ │ lsp)     │ │          │ │            │  │
│  └────┬────┘ └────┬────┘ └────┬─────┘ └────┬─────┘ └─────┬──────┘  │
│       │           │           │             │             │          │
├───────┴───────────┴───────────┴─────────────┴─────────────┴──────────┤
│                    rivet-core (Pure Rust Library)                     │
│                                                                       │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                    Analysis Coordinator                       │    │
│  │    (Orchestrates parsing → metrics → output pipeline)         │    │
│  └───────┬───────────────────┬──────────────────┬───────────┘    │
│          │                   │                  │                 │
│  ┌───────▼───────┐  ┌───────▼──────┐  ┌───────▼──────────┐     │
│  │  Parser Layer │  │Metrics Engine│  │  Output Layer    │     │
│  │  (tree-sitter)│  │ (trait-based)│  │ (JSON/SARIF/CSV) │     │
│  └───────┬───────┘  └───────┬──────┘  └──────────────────┘     │
│          │                  │                                    │
│  ┌───────▼───────┐  ┌──────▼───────┐                            │
│  │   Language    │  │    Plugin    │                             │
│  │   Registry   │  │   Host       │                             │
│  │ (170+ langs) │  │ (Extism/WASM)│                             │
│  └──────────────┘  └──────────────┘                             │
└──────────────────────────────────────────────────────────────────┘
```

### 3.2 Crate Dependency Graph

```text
rivet (workspace)
│
├── rivet-core          ← Pure library. Zero IO. No async runtime.
│   ├── tree-sitter
│   ├── tree-sitter-{language} (feature-gated)
│   ├── extism           (optional, feature = "plugins")
│   ├── serde / serde_json
│   └── rayon            (parallel file analysis)
│
├── rivet-cli           ← Binary. Depends on rivet-core.
│   ├── rivet-core
│   ├── clap v4
│   ├── ignore           (gitignore-aware file walking)
│   └── tracing / tracing-subscriber
│
├── rivet-mcp           ← MCP server binary. Depends on rivet-core.
│   ├── rivet-core
│   ├── rmcp             (official MCP Rust SDK)
│   ├── tokio
│   └── schemars
│
├── rivet-lsp           ← LSP server binary. Depends on rivet-core.
│   ├── rivet-core
│   ├── tower-lsp-server (community-maintained tower-lsp fork)
│   ├── tokio
│   ├── dashmap          (concurrent document state)
│   └── tracing
│
├── rivet-python        ← PyO3 binding crate. cdylib.
│   ├── rivet-core
│   └── pyo3
│
├── rivet-node          ← napi-rs binding crate. cdylib.
│   ├── rivet-core
│   └── napi / napi-derive
│
└── rivet-plugin-sdk    ← For plugin authors. Compiles to wasm32.
    └── extism-pdk
```

### 3.3 Design Principles

1. **Core is pure.** `rivet-core` has no IO, no async runtime, no CLI framework. It takes `&[u8]` source code and returns structured results. This makes it trivially embeddable and testable.

2. **Consumers own the IO.** The CLI handles file walking and output writing. The MCP server handles transport. The bindings handle FFI marshalling. None of this logic leaks into the core.

3. **Traits define extension points.** `MetricAnalyzer`, `OutputFormatter`, and `LanguageProvider` are the three extension traits. Built-in implementations use static dispatch; plugins use dynamic dispatch via WASM.

4. **Feature flags control binary size.** Language grammars are feature-gated. A user analyzing only Python/TypeScript/Rust need not compile all 170 grammars.

5. **Errors are values, not panics.** All fallible operations return `Result<T, RivetError>`. The core never panics on invalid input — it degrades gracefully (e.g., partial metrics for files with parse errors).

---

## 4. Core Components

### 4.1 Analysis Coordinator

The `Analyzer` struct is the primary entry point for all consumers. It orchestrates the parse → analyze → format pipeline.

```rust
// rivet-core/src/analyzer.rs

pub struct Analyzer {
    language_registry: LanguageRegistry,
    metric_registry: MetricRegistry,
    plugin_host: Option<PluginHost>,
    config: AnalyzerConfig,
}

impl Analyzer {
    /// Create a new Analyzer with default built-in metrics and languages.
    pub fn new(config: AnalyzerConfig) -> Result<Self, RivetError>;

    /// Analyze a single file. Core entry point.
    pub fn analyze_source(
        &self,
        source: &[u8],
        language: Language,
        file_path: Option<&Path>,
    ) -> Result<FileAnalysis, RivetError>;

    /// Analyze multiple files in parallel using rayon.
    pub fn analyze_files(
        &self,
        files: &[FileInput],
    ) -> Result<ProjectAnalysis, RivetError>;

    /// Check if analysis results pass configured thresholds.
    pub fn check_thresholds(
        &self,
        analysis: &ProjectAnalysis,
    ) -> ThresholdResult;

    /// Register a WASM plugin.
    pub fn register_plugin(&mut self, wasm_bytes: &[u8]) -> Result<(), RivetError>;
}
```

**Design rationale:** The `Analyzer` is `Send + Sync`, meaning it can be safely shared across threads (for rayon parallelism) and across async tasks (for the MCP server). The `PluginHost` uses interior mutability (`Arc<Mutex<...>>`) only where WASM state requires it.

### 4.2 Language Registry

Maps file extensions and language identifiers to tree-sitter grammar configurations.

```rust
// rivet-core/src/language.rs

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    Rust,
    Python,
    TypeScript,
    JavaScript,
    Go,
    Java,
    Cpp,
    CSharp,
    Ruby,
    // ... 170+ variants, feature-gated
    Custom(u32),  // For plugin-provided languages
}

pub struct LanguageRegistry {
    languages: HashMap<Language, LanguageConfig>,
    extension_map: HashMap<String, Language>,
}

pub struct LanguageConfig {
    /// The tree-sitter Language object.
    pub grammar: tree_sitter::Language,
    /// tree-sitter query for identifying function-like nodes.
    pub function_query: tree_sitter::Query,
    /// tree-sitter query for identifying class/module-like nodes.
    pub scope_query: tree_sitter::Query,
    /// tree-sitter query for identifying control flow nodes (if/for/while/match).
    pub control_flow_query: tree_sitter::Query,
    /// tree-sitter query for identifying operators (for Halstead).
    pub operator_query: tree_sitter::Query,
    /// tree-sitter query for identifying operands (for Halstead).
    pub operand_query: tree_sitter::Query,
    /// Language-specific comment node types.
    pub comment_node_types: Vec<String>,
}
```

**Key insight:** All language-specific behavior is encoded in **tree-sitter queries**, not in Rust code. Adding a new language requires only writing `.scm` query files, not modifying Rust source. This is the core extensibility mechanism — far more maintainable than Lizard's per-language Python files.

### 4.3 Metric Registry

Manages both built-in and plugin-provided metric analyzers.

```rust
// rivet-core/src/metrics/registry.rs

pub struct MetricRegistry {
    analyzers: Vec<Box<dyn MetricAnalyzer>>,
    plugin_analyzers: Vec<PluginMetricAnalyzer>,  // WASM-backed
}

impl MetricRegistry {
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();
        registry.register(Box::new(CyclomaticComplexity));
        registry.register(Box::new(CognitiveComplexity));
        registry.register(Box::new(HalsteadMetrics));
        registry.register(Box::new(LinesOfCode));
        registry.register(Box::new(ParameterCount));
        registry.register(Box::new(MaintainabilityIndex));
        registry
    }

    pub fn analyze_all(
        &self,
        tree: &tree_sitter::Tree,
        source: &[u8],
        lang_config: &LanguageConfig,
    ) -> Vec<MetricResult>;
}
```

---

## 5. Type System and Data Models

### 5.1 Core Result Types

```rust
// rivet-core/src/types.rs

/// Top-level result for a single file analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileAnalysis {
    pub file_path: Option<PathBuf>,
    pub language: Language,
    pub file_metrics: FileMetrics,
    pub functions: Vec<FunctionAnalysis>,
    pub parse_errors: Vec<ParseError>,
    pub analysis_duration: Duration,
}

/// Aggregate metrics for the entire file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMetrics {
    pub nloc: u32,           // Non-comment lines of code
    pub sloc: u32,           // Source lines of code
    pub ploc: u32,           // Physical lines of code
    pub lloc: u32,           // Logical lines of code
    pub cloc: u32,           // Comment lines of code
    pub blank: u32,          // Blank lines
    pub total_complexity: f64,
    pub avg_complexity: f64,
    pub max_complexity: f64,
    pub maintainability_index: f64,
    pub halstead: HalsteadFileMetrics,
}

/// Per-function analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionAnalysis {
    pub name: String,
    pub qualified_name: String,    // e.g., "MyClass.my_method"
    pub start_line: u32,
    pub end_line: u32,
    pub start_column: u32,
    pub end_column: u32,
    pub cyclomatic_complexity: u32,
    pub cognitive_complexity: u32,
    pub parameter_count: u32,
    pub token_count: u32,
    pub nloc: u32,
    pub halstead: HalsteadFunctionMetrics,
    pub nesting_depth: u32,        // Maximum nesting depth
}

/// Halstead metric suite (per function or per file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HalsteadFunctionMetrics {
    pub n1: u32,    // Number of distinct operators
    pub n2: u32,    // Number of distinct operands
    pub big_n1: u32, // Total number of operators
    pub big_n2: u32, // Total number of operands
    pub vocabulary: u32,
    pub length: u32,
    pub calculated_length: f64,
    pub volume: f64,
    pub difficulty: f64,
    pub effort: f64,
    pub time: f64,          // Time to program (seconds)
    pub bugs: f64,          // Estimated bugs
}

/// Aggregated project-level result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectAnalysis {
    pub files: Vec<FileAnalysis>,
    pub summary: ProjectSummary,
    pub threshold_violations: Vec<ThresholdViolation>,
}

/// Project-wide summary statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSummary {
    pub total_files: u32,
    pub total_functions: u32,
    pub total_nloc: u32,
    pub avg_cyclomatic: f64,
    pub avg_cognitive: f64,
    pub avg_maintainability_index: f64,
    pub languages: HashMap<Language, LanguageSummary>,
}
```

### 5.2 Threshold Configuration

```rust
/// Configurable quality gates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Thresholds {
    /// Maximum cyclomatic complexity per function.
    pub max_cyclomatic_complexity: Option<u32>,    // Default: 15
    /// Maximum cognitive complexity per function.
    pub max_cognitive_complexity: Option<u32>,      // Default: 15
    /// Maximum function length in NLOC.
    pub max_function_length: Option<u32>,           // Default: 100
    /// Maximum parameter count per function.
    pub max_parameter_count: Option<u32>,           // Default: 5
    /// Maximum nesting depth per function.
    pub max_nesting_depth: Option<u32>,             // Default: 5
    /// Minimum maintainability index per file.
    pub min_maintainability_index: Option<f64>,     // Default: 20.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdViolation {
    pub file_path: Option<PathBuf>,
    pub function_name: String,
    pub metric_name: String,
    pub actual_value: f64,
    pub threshold_value: f64,
    pub severity: Severity,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Severity {
    Warning,
    Error,
}
```

---

## 6. Parser Layer

### 6.1 tree-sitter Integration Strategy

The parser layer wraps tree-sitter to provide a consistent interface for all metrics. The key design decision is that **all language-specific logic lives in tree-sitter query files (`.scm`), not in Rust code**.

```rust
// rivet-core/src/parser/mod.rs

pub struct Parser {
    inner: tree_sitter::Parser,
}

impl Parser {
    pub fn new() -> Self;

    pub fn parse(
        &mut self,
        source: &[u8],
        language: &LanguageConfig,
    ) -> Result<ParseResult, RivetError>;
}

pub struct ParseResult {
    pub tree: tree_sitter::Tree,
    pub errors: Vec<ParseError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParseError {
    pub start_line: u32,
    pub start_column: u32,
    pub end_line: u32,
    pub end_column: u32,
    pub message: String,
}
```

**Note on `Parser` thread-safety:** tree-sitter's `Parser` is `!Send` (it uses internal C state). Each thread in the rayon pool creates its own `Parser` instance via `thread_local!`. The parsed `Tree` is `Send`, so it can be moved between threads after parsing.

### 6.2 Language Query Architecture

Each supported language has a directory of `.scm` query files:

```text
queries/
├── rust/
│   ├── functions.scm        # Matches function_item, impl method, closure
│   ├── control_flow.scm     # Matches if, for, while, loop, match arms
│   ├── operators.scm        # Matches binary_expression operators
│   ├── operands.scm         # Matches identifiers, literals
│   └── scopes.scm           # Matches impl, mod, trait blocks
├── python/
│   ├── functions.scm
│   ├── control_flow.scm
│   ├── operators.scm
│   ├── operands.scm
│   └── scopes.scm
├── typescript/
│   └── ...
└── ...
```

Example `functions.scm` for Rust:

```scheme
;; Match function definitions
(function_item
  name: (identifier) @function.name) @function.def

;; Match methods in impl blocks
(impl_item
  body: (declaration_list
    (function_item
      name: (identifier) @method.name) @method.def))

;; Match closures assigned to variables
(let_declaration
  pattern: (identifier) @closure.name
  value: (closure_expression) @closure.def)
```

Example `control_flow.scm` for Rust:

```scheme
;; Each match increments cyclomatic complexity by 1
(if_expression) @cc.branch
(else_clause) @cc.branch
(while_expression) @cc.branch
(for_expression) @cc.branch
(loop_expression) @cc.branch
(match_arm) @cc.branch            ;; Each arm is a branch
(binary_expression
  operator: ["&&" "||"]) @cc.boolean_op
```

### 6.3 Feature-Gated Language Support

```toml
# rivet-core/Cargo.toml
[features]
default = ["lang-popular"]  # ~20 most-used languages
lang-popular = [
    "lang-rust", "lang-python", "lang-typescript", "lang-javascript",
    "lang-go", "lang-java", "lang-cpp", "lang-c", "lang-csharp",
    "lang-ruby", "lang-php", "lang-swift", "lang-kotlin",
    "lang-scala", "lang-lua", "lang-r", "lang-sql",
    "lang-hcl", "lang-yaml", "lang-toml",
]
lang-all = [...]  # All 170+ languages
lang-rust = ["dep:tree-sitter-rust"]
lang-python = ["dep:tree-sitter-python"]
# ... etc
```

This keeps compile times and binary sizes manageable. A minimal build with just Rust + Python support compiles in ~30s; `lang-all` may take several minutes.

---

## 7. Metrics Engine

### 7.1 MetricAnalyzer Trait

```rust
// rivet-core/src/metrics/trait.rs

/// The core extension point for all metrics — both built-in and plugin.
pub trait MetricAnalyzer: Send + Sync {
    /// Unique identifier for this metric (e.g., "cyclomatic_complexity").
    fn id(&self) -> &str;

    /// Human-readable name (e.g., "Cyclomatic Complexity").
    fn display_name(&self) -> &str;

    /// Analyze a single function node in the AST.
    fn analyze_function(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        lang_config: &LanguageConfig,
    ) -> Result<MetricValue, RivetError>;

    /// Optionally analyze file-level metrics.
    fn analyze_file(
        &self,
        tree: &tree_sitter::Tree,
        source: &[u8],
        lang_config: &LanguageConfig,
    ) -> Result<Option<MetricValue>, RivetError> {
        Ok(None)  // Default: no file-level metric
    }

    /// JSON schema describing this metric's output shape.
    fn output_schema(&self) -> serde_json::Value;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Float(f64),
    Composite(HashMap<String, MetricValue>),
}
```

### 7.2 Built-in Metric Implementations

#### 7.2.1 Cyclomatic Complexity (McCabe)

```text
CC = E - N + 2P
```

Where E = edges, N = nodes, P = connected components. In practice, for a single function:

```text
CC = 1 + (number of branching points)
```

Branching points are identified via the `control_flow.scm` query:

- `if`, `elif`, `else if` → +1 each
- `for`, `while`, `loop` → +1 each
- `case`, `match arm` → +1 each
- `&&`, `||` → +1 each
- `catch`, `except` → +1 each
- Ternary operator → +1

```rust
// rivet-core/src/metrics/cyclomatic.rs

pub struct CyclomaticComplexity;

impl MetricAnalyzer for CyclomaticComplexity {
    fn id(&self) -> &str { "cyclomatic_complexity" }
    fn display_name(&self) -> &str { "Cyclomatic Complexity" }

    fn analyze_function(
        &self,
        node: &tree_sitter::Node,
        source: &[u8],
        lang_config: &LanguageConfig,
    ) -> Result<MetricValue, RivetError> {
        let mut cursor = tree_sitter::QueryCursor::new();
        let matches = cursor.matches(
            &lang_config.control_flow_query,
            *node,
            source,
        );
        // Base complexity is 1 (the function itself is one path)
        let cc = 1 + matches.count() as i64;
        Ok(MetricValue::Integer(cc))
    }
}
```

#### 7.2.2 Cognitive Complexity (SonarSource)

Cognitive complexity differs from cyclomatic in three key ways:

1. **Nesting increments**: Each level of nesting adds +1 to the increment for control flow structures inside it.
2. **No increment for `else`**: Unlike CC, `else` does not add to cognitive complexity.
3. **Boolean operator sequences**: Only changes in operator type (`&&` to `||` or vice versa) increment; consecutive same-operators do not.

```rust
// rivet-core/src/metrics/cognitive.rs

pub struct CognitiveComplexity;

impl CognitiveComplexity {
    fn walk_cognitive(
        &self,
        node: tree_sitter::Node,
        source: &[u8],
        nesting: u32,
        lang_config: &LanguageConfig,
    ) -> u32 {
        let mut complexity = 0u32;

        // Check if this node is a nesting-incrementing structure
        let is_nesting_structure = Self::is_nesting_increment(&node, lang_config);

        // Check if this node is a complexity-incrementing structure
        if Self::is_structural_increment(&node, lang_config) {
            complexity += 1 + nesting;  // Base increment + nesting penalty
        } else if Self::is_fundamental_increment(&node, lang_config) {
            complexity += 1;  // No nesting penalty for boolean ops etc.
        }

        let child_nesting = if is_nesting_structure { nesting + 1 } else { nesting };

        // Recurse into children
        let mut child_cursor = node.walk();
        for child in node.children(&mut child_cursor) {
            complexity += self.walk_cognitive(child, source, child_nesting, lang_config);
        }

        complexity
    }
}
```

#### 7.2.3 Halstead Metrics

Halstead metrics quantify the "size" and "difficulty" of code based on operators and operands.

| Metric         | Formula          | Description                            |
| :------------- | :--------------- | :------------------------------------- |
| Vocabulary (n) | n1 + n2          | Distinct operators + distinct operands |
| Length (N)     | N1 + N2          | Total operators + total operands       |
| Volume (V)     | N × log₂(n)      | Information content                    |
| Difficulty (D) | (n1/2) × (N2/n2) | Error-proneness                        |
| Effort (E)     | D × V            | Implementation effort                  |
| Time (T)       | E / 18           | Estimated coding time (seconds)        |
| Bugs (B)       | V / 3000         | Estimated bugs                         |

The `operator_query` and `operand_query` tree-sitter queries identify operators and operands per language.

#### 7.2.4 Lines of Code

| Metric | Description                           |
| :----- | :------------------------------------ |
| PLOC   | Physical lines (total line count)     |
| SLOC   | Source lines (non-blank, non-comment) |
| LLOC   | Logical lines (statements)            |
| CLOC   | Comment lines                         |
| BLANK  | Blank lines                           |

LOC metrics are computed by walking the AST and classifying each line based on whether it contains source nodes, comment nodes, or neither.

#### 7.2.5 Maintainability Index

```text
MI = max(0, (171 - 5.2 × ln(V) - 0.23 × CC - 16.2 × ln(SLOC)) × 100 / 171)
```

Where V = Halstead Volume, CC = Cyclomatic Complexity, SLOC = Source Lines of Code. The result is normalized to 0–100 where higher is more maintainable.

---

## 8. Plugin System

### 8.1 Architecture Overview

The plugin system uses Extism, a WASM-based framework, to provide sandboxed extensibility. Plugin authors write custom metrics in any language that compiles to WASM (Rust, Go, AssemblyScript, C, Zig) and distribute them as `.wasm` files.

```text
┌─────────────────────────┐        ┌──────────────────────────┐
│    rivet-core host     │        │   Custom Plugin (.wasm)  │
│                         │        │                          │
│  PluginHost             │  WASM  │  fn analyze(input: JSON) │
│    → Extism::Plugin ────┼───────►│    → output: JSON        │
│    → sandbox: memory,   │        │                          │
│      CPU limits, no FS  │        │  Written in Rust/Go/     │
│                         │        │  AssemblyScript/C/Zig    │
└─────────────────────────┘        └──────────────────────────┘
```

### 8.2 Plugin Interface Contract

Plugins must export a single function:

```rust
// rivet-plugin-sdk/src/lib.rs  (compiled to wasm32-unknown-unknown)

use extism_pdk::*;
use serde::{Deserialize, Serialize};

/// Input provided by the host to the plugin.
#[derive(Deserialize)]
pub struct AnalyzeInput {
    /// The source code of the function being analyzed.
    pub source: String,
    /// The function name.
    pub function_name: String,
    /// The language identifier.
    pub language: String,
    /// The AST in S-expression format.
    pub sexp: String,
    /// Start and end lines.
    pub start_line: u32,
    pub end_line: u32,
}

/// Output returned by the plugin.
#[derive(Serialize)]
pub struct AnalyzeOutput {
    /// Metric identifier (e.g., "my_custom_metric").
    pub metric_id: String,
    /// Display name (e.g., "My Custom Metric").
    pub display_name: String,
    /// The computed metric value.
    pub value: MetricValue,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum MetricValue {
    Integer(i64),
    Float(f64),
    Composite(std::collections::HashMap<String, MetricValue>),
}

/// Plugin authors implement this function.
#[plugin_fn]
pub fn analyze(input: String) -> FnResult<String> {
    let input: AnalyzeInput = serde_json::from_str(&input)?;

    // Custom metric logic here...
    let value = compute_my_metric(&input);

    let output = AnalyzeOutput {
        metric_id: "my_custom_metric".into(),
        display_name: "My Custom Metric".into(),
        value: MetricValue::Integer(value),
    };

    Ok(serde_json::to_string(&output)?)
}
```

### 8.3 Plugin Host Configuration

```rust
// rivet-core/src/plugin/host.rs

pub struct PluginHost {
    plugins: Vec<LoadedPlugin>,
}

pub struct LoadedPlugin {
    plugin: extism::Plugin,
    manifest: PluginManifest,
}

#[derive(Debug, Deserialize)]
pub struct PluginManifest {
    pub name: String,
    pub version: String,
    pub metrics: Vec<String>,
    pub supported_languages: Vec<String>,  // Empty = all languages
}

impl PluginHost {
    pub fn load_plugin(
        &mut self,
        wasm_bytes: &[u8],
        config: PluginConfig,
    ) -> Result<(), RivetError> {
        let manifest = extism::Manifest::new([extism::Wasm::data(wasm_bytes)])
            .with_memory_max_pages(config.max_memory_pages)  // Memory limit
            .with_timeout(config.timeout);                    // CPU time limit

        let plugin = extism::Plugin::new(&manifest, [], true)?;
        // ... register
        Ok(())
    }
}

pub struct PluginConfig {
    pub max_memory_pages: u32,    // Default: 256 (16MB)
    pub timeout: Duration,         // Default: 5s per call
}
```

### 8.4 Plugin Discovery

Plugins are discovered from:

1. **CLI flag:** `--plugin ./my_plugin.wasm`
2. **Config file:** `plugins` section in `rivet.toml`
3. **Well-known directory:** `~/.rivet/plugins/`
4. **Project-local:** `.rivet/plugins/` in the project root

---

## 9. Language Bindings

### 9.1 Python Bindings (PyO3 + maturin)

The Python bindings expose `rivet-core` as a native Python module with full type annotations.

```rust
// rivet-python/src/lib.rs

use pyo3::prelude::*;
use rivet_core::{Analyzer, AnalyzerConfig, Language};

#[pyclass]
#[derive(Clone)]
struct PyAnalyzer {
    inner: Analyzer,
}

#[pymethods]
impl PyAnalyzer {
    #[new]
    #[pyo3(signature = (config=None))]
    fn new(config: Option<PyAnalyzerConfig>) -> PyResult<Self> {
        let config = config.map(|c| c.into()).unwrap_or_default();
        Ok(Self {
            inner: Analyzer::new(config).map_err(to_py_err)?,
        })
    }

    /// Analyze a source code string.
    #[pyo3(signature = (source, language, file_path=None))]
    fn analyze_source(
        &self,
        source: &str,
        language: &str,
        file_path: Option<&str>,
    ) -> PyResult<PyFileAnalysis> {
        let lang = Language::from_str(language).map_err(to_py_err)?;
        let result = self.inner
            .analyze_source(source.as_bytes(), lang, file_path.map(Path::new))
            .map_err(to_py_err)?;
        Ok(result.into())
    }

    /// Analyze a directory of files.
    fn analyze_directory(&self, path: &str, glob: Option<&str>) -> PyResult<PyProjectAnalysis> {
        // ...
    }

    /// Check thresholds against analysis results.
    fn check_thresholds(&self, analysis: &PyProjectAnalysis) -> PyResult<PyThresholdResult> {
        // ...
    }
}

#[pyclass]
struct PyFileAnalysis {
    #[pyo3(get)]
    file_path: Option<String>,
    #[pyo3(get)]
    language: String,
    #[pyo3(get)]
    functions: Vec<PyFunctionAnalysis>,
    // ...
}

#[pymodule]
fn rivet_rs(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyAnalyzer>()?;
    m.add_class::<PyFileAnalysis>()?;
    m.add_class::<PyFunctionAnalysis>()?;
    m.add_class::<PyProjectAnalysis>()?;
    Ok(())
}
```

**Python usage:**

```python
from rivet_rs import Analyzer

analyzer = Analyzer(config={"thresholds": {"max_cyclomatic_complexity": 10}})

# Analyze a string
result = analyzer.analyze_source(
    source="def foo(x):\n  if x > 0:\n    return x\n  return -x",
    language="python",
)

for func in result.functions:
    print(f"{func.name}: CC={func.cyclomatic_complexity}")

# Analyze a directory
project = analyzer.analyze_directory("./src", glob="**/*.py")
violations = analyzer.check_thresholds(project)
```

**Build and publish:**

```bash
# Development
maturin develop --release

# Build wheels for distribution
maturin build --release --strip

# Publish to PyPI
maturin publish
```

### 9.2 Node.js / TypeScript Bindings (napi-rs)

```rust
// rivet-node/src/lib.rs

use napi::bindgen_prelude::*;
use napi_derive::napi;
use rivet_core::{Analyzer, AnalyzerConfig, Language};

#[napi(object)]
pub struct AnalyzerOptions {
    pub max_cyclomatic_complexity: Option<u32>,
    pub max_cognitive_complexity: Option<u32>,
    pub max_function_length: Option<u32>,
    pub max_parameter_count: Option<u32>,
}

#[napi]
pub struct JsAnalyzer {
    inner: Analyzer,
}

#[napi]
impl JsAnalyzer {
    #[napi(constructor)]
    pub fn new(options: Option<AnalyzerOptions>) -> Result<Self> {
        let config = options.map(|o| o.into()).unwrap_or_default();
        Ok(Self {
            inner: Analyzer::new(config).map_err(to_napi_err)?,
        })
    }

    /// Analyze a source code string.
    #[napi]
    pub fn analyze_source(
        &self,
        source: String,
        language: String,
        file_path: Option<String>,
    ) -> Result<JsFileAnalysis> {
        let lang = Language::from_str(&language).map_err(to_napi_err)?;
        let result = self.inner
            .analyze_source(source.as_bytes(), lang, file_path.as_deref().map(Path::new))
            .map_err(to_napi_err)?;
        Ok(result.into())
    }

    /// Analyze a directory (returns a Promise for async usage).
    #[napi]
    pub async fn analyze_directory(
        &self,
        path: String,
        glob: Option<String>,
    ) -> Result<JsProjectAnalysis> {
        // Offload to blocking thread pool
        // ...
    }
}
```

**Auto-generated TypeScript definitions (by napi-rs):**

```typescript
// index.d.ts (auto-generated)

export interface AnalyzerOptions {
  maxCyclomaticComplexity?: number;
  maxCognitiveComplexity?: number;
  maxFunctionLength?: number;
  maxParameterCount?: number;
}

export class Analyzer {
  constructor(options?: AnalyzerOptions);
  analyzeSource(
    source: string,
    language: string,
    filePath?: string,
  ): FileAnalysis;
  analyzeDirectory(path: string, glob?: string): Promise<ProjectAnalysis>;
  checkThresholds(analysis: ProjectAnalysis): ThresholdResult;
}

export interface FileAnalysis {
  filePath?: string;
  language: string;
  functions: FunctionAnalysis[];
  fileMetrics: FileMetrics;
}

export interface FunctionAnalysis {
  name: string;
  qualifiedName: string;
  startLine: number;
  endLine: number;
  cyclomaticComplexity: number;
  cognitiveComplexity: number;
  parameterCount: number;
  tokenCount: number;
  nloc: number;
}
```

**TypeScript usage:**

```typescript
import { Analyzer } from "@rivet-rs/node";

const analyzer = new Analyzer({ maxCyclomaticComplexity: 10 });

const result = analyzer.analyzeSource(
  `function foo(x: number): number {
    if (x > 0) { return x; }
    return -x;
  }`,
  "typescript",
);

for (const fn of result.functions) {
  console.log(`${fn.name}: CC=${fn.cyclomaticComplexity}`);
}
```

---

## 10. AI Agent Integration

### 10.1 MCP Server (`rivet-mcp`)

The MCP server is the primary integration point for AI coding agents. It uses the official `rmcp` crate (v0.16+) with stdio transport.

```rust
// rivet-mcp/src/main.rs

use rmcp::{
    ServerHandler, ServiceExt,
    handler::server::tool::ToolRouter,
    model::*,
    tool, tool_handler, tool_router,
    transport::stdio,
};
use schemars;
use serde::Deserialize;
use rivet_core::{Analyzer, AnalyzerConfig};

#[derive(Clone)]
pub struct RivetMcpServer {
    analyzer: Analyzer,
    tool_router: ToolRouter<Self>,
}

// ── Tool Parameter Schemas ──────────────────────────────────────

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeFileParams {
    #[schemars(description = "Path to the file to analyze")]
    pub path: String,
    #[schemars(description = "Override language detection (e.g., 'rust', 'python')")]
    pub language: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeDirectoryParams {
    #[schemars(description = "Path to the directory to analyze")]
    pub path: String,
    #[schemars(description = "Glob pattern for file matching (default: all supported)")]
    pub glob: Option<String>,
    #[schemars(description = "Output format: 'json' or 'sarif'")]
    pub format: Option<String>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct CheckThresholdsParams {
    #[schemars(description = "Path to file or directory to check")]
    pub path: String,
    #[schemars(description = "Maximum cyclomatic complexity per function")]
    pub max_cc: Option<u32>,
    #[schemars(description = "Maximum cognitive complexity per function")]
    pub max_cognitive: Option<u32>,
    #[schemars(description = "Maximum function length in lines")]
    pub max_length: Option<u32>,
    #[schemars(description = "Maximum parameter count")]
    pub max_params: Option<u32>,
}

#[derive(Debug, Deserialize, schemars::JsonSchema)]
pub struct AnalyzeSourceParams {
    #[schemars(description = "Source code to analyze")]
    pub source: String,
    #[schemars(description = "Programming language (e.g., 'rust', 'python', 'typescript')")]
    pub language: String,
}

// ── Tool Implementations ────────────────────────────────────────

#[tool_router]
impl RivetMcpServer {
    pub fn new(config: AnalyzerConfig) -> Self {
        Self {
            analyzer: Analyzer::new(config).expect("Failed to create analyzer"),
            tool_router: Self::tool_router(),
        }
    }

    #[tool(description = "Analyze code complexity metrics for a single file. \
        Returns cyclomatic complexity, cognitive complexity, Halstead metrics, \
        LOC, and per-function breakdowns.")]
    async fn analyze_file(
        &self,
        #[tool(aggr)] params: AnalyzeFileParams,
    ) -> Result<CallToolResult, ErrorData> {
        let source = std::fs::read(&params.path)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;
        let language = params.language
            .as_deref()
            .map(|l| Language::from_str(l))
            .unwrap_or_else(|| Language::from_path(&params.path))
            .map_err(|e| ErrorData::invalid_params(e.to_string(), None))?;

        let result = self.analyzer
            .analyze_source(&source, language, Some(Path::new(&params.path)))
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        let json = serde_json::to_string_pretty(&result)
            .map_err(|e| ErrorData::internal_error(e.to_string(), None))?;

        Ok(CallToolResult::success(vec![Content::text(json)]))
    }

    #[tool(description = "Analyze all source files in a directory. \
        Returns aggregated metrics with per-file and per-function details. \
        Supports glob filtering.")]
    async fn analyze_directory(
        &self,
        #[tool(aggr)] params: AnalyzeDirectoryParams,
    ) -> Result<CallToolResult, ErrorData> {
        // ... walk directory, analyze files, return results
        todo!()
    }

    #[tool(description = "Check if code meets complexity thresholds. \
        Returns pass/fail with list of violations. \
        Use this before submitting PRs to enforce quality gates.")]
    async fn check_thresholds(
        &self,
        #[tool(aggr)] params: CheckThresholdsParams,
    ) -> Result<CallToolResult, ErrorData> {
        // ... analyze, check thresholds, return pass/fail
        todo!()
    }

    #[tool(description = "Analyze a code snippet directly (no file needed). \
        Provide the source code and language identifier.")]
    async fn analyze_source(
        &self,
        #[tool(aggr)] params: AnalyzeSourceParams,
    ) -> Result<CallToolResult, ErrorData> {
        // ... analyze source string
        todo!()
    }
}

#[tool_handler]
impl ServerHandler for RivetMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2025_06_18,
            capabilities: ServerCapabilities::builder()
                .enable_tools()
                .build(),
            server_info: Implementation {
                name: "rivet".into(),
                version: env!("CARGO_PKG_VERSION").into(),
            },
            instructions: Some(
                "Code complexity analysis server. Use analyze_file or \
                 analyze_source to get metrics. Use check_thresholds to \
                 enforce quality gates before PR submission.".into()
            ),
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let server = RivetMcpServer::new(AnalyzerConfig::default());
    let service = server.serve(stdio()).await?;
    service.waiting().await?;
    Ok(())
}
```

**MCP client configuration (Claude Code / `~/.claude/settings.json`):**

```json
{
  "mcpServers": {
    "rivet": {
      "command": "rivet-mcp",
      "args": [],
      "env": {}
    }
  }
}
```

**MCP client configuration (Cursor / `.cursor/mcp.json`):**

```json
{
  "mcpServers": {
    "rivet": {
      "command": "rivet-mcp",
      "args": [],
      "transportType": "stdio"
    }
  }
}
```

### 10.2 Agent Skill Integration (`CLAUDE.md` pattern)

For projects using `meta-agent-skills` or similar agent configuration frameworks, rivet should be referenced as a skill:

````markdown
## Skill: Code Complexity Check

Before submitting any PR, run complexity analysis:

```bash
rivet check ./src --max-cc 15 --max-cognitive 15 --format sarif
```
````

If any violations are found, refactor the offending functions before proceeding.
Alternatively, use the MCP tool `check_thresholds` if the rivet MCP server is configured.

### 10.3 SARIF for GitHub Code Scanning

The SARIF output enables direct integration with GitHub Advanced Security:

```yaml
# .github/workflows/complexity.yml
name: Code Complexity
on: [push, pull_request]
jobs:
  analyze:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install rivet
        run: cargo install rivet-cli
      - name: Run complexity analysis
        run: rivet analyze ./src --format sarif --output results.sarif --max-cc 15
      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v3
        with:
          sarif_file: results.sarif
```

---

## 11. LSP Server

### 11.1 Rationale

The LSP server provides the **tightest possible feedback loop** for developers — complexity metrics appear as diagnostics, code lenses, and hover information directly in the editor, updated on every keystroke. While the MCP server enables AI agents to invoke analysis programmatically, the LSP server enables humans (and AI agents operating within editors like Cursor or Zed) to see complexity signals in real-time without leaving their workflow.

The LSP and MCP servers are complementary, not competing:

| Dimension               | MCP Server                | LSP Server                                     |
| :---------------------- | :------------------------ | :--------------------------------------------- |
| **Primary consumer**    | AI coding agents          | Human developers + editor-embedded agents      |
| **Invocation**          | On-demand tool calls      | Continuous, event-driven (on open/save/change) |
| **Output**              | Structured JSON results   | Diagnostics, code lenses, hover overlays       |
| **Transport**           | stdio (MCP protocol)      | stdio or TCP (LSP/JSON-RPC)                    |
| **Latency requirement** | Hundreds of ms acceptable | Sub-100ms for perceived real-time              |

### 11.2 Technology Choice: `tower-lsp-server`

The LSP server is built on `tower-lsp-server` (the community-maintained fork of `tower-lsp`), which provides:

- Async `LanguageServer` trait with method-per-LSP-request design
- Built-in stdio and TCP transport
- Tokio-based async runtime
- `lsp-types` v0.97 for full LSP 3.17 specification coverage
- Client notification push (for publishing diagnostics)

**Why `tower-lsp-server` over the original `tower-lsp`?** The community fork (`tower-lsp-community/tower-lsp-server`) has active maintenance, uses the newer `impl Trait in Trait` feature (no `#[async_trait]` needed), and ships with updated `ls-types` that track the latest LSP specification.

### 11.3 Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│                        rivet-lsp                                │
│                                                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │               RivetLanguageServer                         │  │
│  │  impl LanguageServer for RivetLanguageServer              │  │
│  └────────┬──────────┬──────────┬──────────┬─────────────────┘  │
│           │          │          │          │                      │
│  ┌────────▼───┐ ┌────▼────┐ ┌──▼────┐ ┌──▼──────────────────┐  │
│  │  Document  │ │Diagnostic│ │ Code  │ │  Hover Provider    │  │
│  │  State     │ │Publisher │ │ Lens  │ │  (per-function     │  │
│  │ (DashMap)  │ │         │ │Provider│ │   metric summary)  │  │
│  └────────────┘ └─────────┘ └───────┘ └──────────────────────┘  │
│           │                                                       │
│  ┌────────▼─────────────────────────────────────────────────┐   │
│  │              rivet-core::Analyzer                         │   │
│  │   (Shared, immutable after construction — Send + Sync)     │   │
│  └───────────────────────────────────────────────────────────┘   │
└──────────────────────────────────────────────────────────────────┘
```

### 11.4 Document State Management

The LSP server maintains an in-memory map of open documents and their latest analysis results. This enables incremental re-analysis on each text change without re-reading files from disk.

```rust
// rivet-lsp/src/state.rs

use dashmap::DashMap;
use rivet_core::{FileAnalysis, Language};
use std::sync::Arc;

/// Thread-safe document state shared across LSP request handlers.
#[derive(Clone)]
pub struct DocumentState {
    /// Map from document URI to the latest analysis result.
    documents: Arc<DashMap<String, DocumentEntry>>,
}

pub struct DocumentEntry {
    /// The latest source text (received via textDocument/didChange).
    pub source: String,
    /// The detected or overridden language.
    pub language: Language,
    /// The latest analysis result (None if analysis is pending/failed).
    pub analysis: Option<FileAnalysis>,
    /// Version number from the LSP client.
    pub version: i32,
}

impl DocumentState {
    pub fn new() -> Self {
        Self { documents: Arc::new(DashMap::new()) }
    }

    pub fn update_source(&self, uri: &str, source: String, version: i32, language: Language) {
        self.documents.insert(uri.to_string(), DocumentEntry {
            source,
            language,
            analysis: None,
            version,
        });
    }

    pub fn update_analysis(&self, uri: &str, analysis: FileAnalysis) {
        if let Some(mut entry) = self.documents.get_mut(uri) {
            entry.analysis = Some(analysis);
        }
    }

    pub fn remove(&self, uri: &str) {
        self.documents.remove(uri);
    }

    pub fn get_analysis(&self, uri: &str) -> Option<FileAnalysis> {
        self.documents.get(uri)?.analysis.clone()
    }
}
```

### 11.5 LanguageServer Implementation

```rust
// rivet-lsp/src/server.rs

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::*;
use tower_lsp_server::{Client, LanguageServer, LspService, Server};
use rivet_core::{Analyzer, AnalyzerConfig, Language};
use crate::state::DocumentState;

#[derive(Clone)]
pub struct RivetLanguageServer {
    client: Client,
    analyzer: Analyzer,
    state: DocumentState,
    config: LspConfig,
}

#[derive(Clone)]
pub struct LspConfig {
    pub thresholds: rivet_core::Thresholds,
    /// Analyze on every keystroke (didChange) vs only on save (didSave).
    pub analyze_on_change: bool,
    /// Show code lenses with per-function metrics.
    pub enable_code_lenses: bool,
    /// Show hover information with metric details.
    pub enable_hover: bool,
}

impl LanguageServer for RivetLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Re-analyze on every change or only on save
                text_document_sync: Some(TextDocumentSyncCapability::Options(
                    TextDocumentSyncOptions {
                        open_close: Some(true),
                        change: Some(if self.config.analyze_on_change {
                            TextDocumentSyncKind::FULL
                        } else {
                            TextDocumentSyncKind::NONE
                        }),
                        save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                            include_text: Some(true),
                        })),
                        ..Default::default()
                    },
                )),
                // Enable code lens (per-function complexity annotations)
                code_lens_provider: if self.config.enable_code_lenses {
                    Some(CodeLensOptions { resolve_provider: Some(false) })
                } else {
                    None
                },
                // Enable hover (detailed metrics on function hover)
                hover_provider: if self.config.enable_hover {
                    Some(HoverProviderCapability::Simple(true))
                } else {
                    None
                },
                // Support code actions (e.g., "suppress this warning")
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                ..Default::default()
            },
            ..Default::default()
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "rivet LSP server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    /// Called when a document is opened — run initial analysis.
    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        let source = params.text_document.text;
        let lang_id = &params.text_document.language_id;

        if let Ok(language) = Language::from_str(lang_id) {
            self.state.update_source(&uri, source.clone(), 0, language);
            self.analyze_and_publish(&uri, &source, language).await;
        }
    }

    /// Called when document content changes — re-analyze.
    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        if !self.config.analyze_on_change { return; }

        let uri = params.text_document.uri.to_string();
        if let Some(change) = params.content_changes.into_iter().last() {
            if let Some(entry) = self.state.documents.get(&uri) {
                let language = entry.language;
                drop(entry);
                self.state.update_source(
                    &uri,
                    change.text.clone(),
                    params.text_document.version,
                    language,
                );
                self.analyze_and_publish(&uri, &change.text, language).await;
            }
        }
    }

    /// Called when document is saved — re-analyze (if not analyzing on change).
    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        if let (Some(text), Some(entry)) = (params.text, self.state.documents.get(&uri)) {
            let language = entry.language;
            drop(entry);
            self.analyze_and_publish(&uri, &text, language).await;
        }
    }

    /// Called when document is closed — clean up state.
    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri.to_string();
        self.state.remove(&uri);
        // Clear diagnostics for this document
        self.client.publish_diagnostics(
            params.text_document.uri,
            vec![],
            None,
        ).await;
    }

    /// Code lenses: show complexity metrics above each function.
    async fn code_lens(&self, params: CodeLensParams) -> Result<Option<Vec<CodeLens>>> {
        let uri = params.text_document.uri.to_string();
        let analysis = match self.state.get_analysis(&uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        let lenses = analysis.functions.iter().map(|func| {
            CodeLens {
                range: Range {
                    start: Position {
                        line: func.start_line.saturating_sub(1),
                        character: 0,
                    },
                    end: Position {
                        line: func.start_line.saturating_sub(1),
                        character: 0,
                    },
                },
                command: Some(Command {
                    title: format!(
                        "CC:{} | Cognitive:{} | Params:{} | NLOC:{}",
                        func.cyclomatic_complexity,
                        func.cognitive_complexity,
                        func.parameter_count,
                        func.nloc,
                    ),
                    command: String::new(),  // Informational only, no action
                    arguments: None,
                }),
                data: None,
            }
        }).collect();

        Ok(Some(lenses))
    }

    /// Hover: show detailed metrics for the function under the cursor.
    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri.to_string();
        let position = params.text_document_position_params.position;

        let analysis = match self.state.get_analysis(&uri) {
            Some(a) => a,
            None => return Ok(None),
        };

        // Find the function containing the cursor position
        let func = analysis.functions.iter().find(|f| {
            let start = f.start_line.saturating_sub(1);
            let end = f.end_line.saturating_sub(1);
            position.line >= start && position.line <= end
        });

        match func {
            Some(func) => {
                let markdown = format!(
                    "### `{}` — Complexity Metrics\n\n\
                     | Metric | Value | Threshold |\n\
                     |:-------|------:|----------:|\n\
                     | Cyclomatic Complexity | **{}** | {} |\n\
                     | Cognitive Complexity | **{}** | {} |\n\
                     | Parameter Count | **{}** | {} |\n\
                     | Lines of Code (NLOC) | **{}** | {} |\n\
                     | Token Count | **{}** | — |\n\
                     | Nesting Depth | **{}** | {} |\n\n\
                     *Halstead Volume:* {:.1} | *Effort:* {:.1} | *Est. Bugs:* {:.2}",
                    func.name,
                    func.cyclomatic_complexity,
                    self.config.thresholds.max_cyclomatic_complexity
                        .map_or("—".into(), |t| t.to_string()),
                    func.cognitive_complexity,
                    self.config.thresholds.max_cognitive_complexity
                        .map_or("—".into(), |t| t.to_string()),
                    func.parameter_count,
                    self.config.thresholds.max_parameter_count
                        .map_or("—".into(), |t| t.to_string()),
                    func.nloc,
                    self.config.thresholds.max_function_length
                        .map_or("—".into(), |t| t.to_string()),
                    func.token_count,
                    func.nesting_depth,
                    self.config.thresholds.max_nesting_depth
                        .map_or("—".into(), |t| t.to_string()),
                    func.halstead.volume,
                    func.halstead.effort,
                    func.halstead.bugs,
                );

                Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: markdown,
                    }),
                    range: Some(Range {
                        start: Position { line: func.start_line.saturating_sub(1), character: 0 },
                        end: Position { line: func.end_line.saturating_sub(1), character: u32::MAX },
                    }),
                }))
            }
            None => Ok(None),
        }
    }
}
```

### 11.6 Diagnostic Publishing

The core analysis-and-publish loop converts threshold violations into LSP diagnostics:

```rust
// rivet-lsp/src/server.rs (continued)

impl RivetLanguageServer {
    /// Run analysis on source and publish diagnostics to the client.
    async fn analyze_and_publish(&self, uri: &str, source: &str, language: Language) {
        let result = match self.analyzer.analyze_source(
            source.as_bytes(), language, None
        ) {
            Ok(r) => r,
            Err(e) => {
                self.client.log_message(
                    MessageType::WARNING,
                    format!("Analysis failed for {}: {}", uri, e),
                ).await;
                return;
            }
        };

        // Convert threshold violations to LSP diagnostics
        let mut diagnostics = Vec::new();

        for func in &result.functions {
            // Cyclomatic complexity
            if let Some(max) = self.config.thresholds.max_cyclomatic_complexity {
                if func.cyclomatic_complexity > max {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: func.start_line.saturating_sub(1),
                                character: func.start_column,
                            },
                            end: Position {
                                line: func.start_line.saturating_sub(1),
                                character: func.start_column + func.name.len() as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("rivet".into()),
                        code: Some(NumberOrString::String("high-cyclomatic-complexity".into())),
                        message: format!(
                            "Function `{}` has cyclomatic complexity {} (threshold: {}). \
                             Consider extracting helper functions or simplifying control flow.",
                            func.name, func.cyclomatic_complexity, max,
                        ),
                        ..Default::default()
                    });
                }
            }

            // Cognitive complexity
            if let Some(max) = self.config.thresholds.max_cognitive_complexity {
                if func.cognitive_complexity > max {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: func.start_line.saturating_sub(1),
                                character: func.start_column,
                            },
                            end: Position {
                                line: func.start_line.saturating_sub(1),
                                character: func.start_column + func.name.len() as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::WARNING),
                        source: Some("rivet".into()),
                        code: Some(NumberOrString::String("high-cognitive-complexity".into())),
                        message: format!(
                            "Function `{}` has cognitive complexity {} (threshold: {}). \
                             Deeply nested logic is hard to understand — flatten or decompose.",
                            func.name, func.cognitive_complexity, max,
                        ),
                        ..Default::default()
                    });
                }
            }

            // Parameter count
            if let Some(max) = self.config.thresholds.max_parameter_count {
                if func.parameter_count > max {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: func.start_line.saturating_sub(1),
                                character: func.start_column,
                            },
                            end: Position {
                                line: func.start_line.saturating_sub(1),
                                character: func.start_column + func.name.len() as u32,
                            },
                        },
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        source: Some("rivet".into()),
                        code: Some(NumberOrString::String("too-many-parameters".into())),
                        message: format!(
                            "Function `{}` has {} parameters (threshold: {}). \
                             Consider using a struct or builder pattern.",
                            func.name, func.parameter_count, max,
                        ),
                        ..Default::default()
                    });
                }
            }

            // Function length
            if let Some(max) = self.config.thresholds.max_function_length {
                if func.nloc > max {
                    diagnostics.push(Diagnostic {
                        range: Range {
                            start: Position {
                                line: func.start_line.saturating_sub(1),
                                character: 0,
                            },
                            end: Position {
                                line: func.end_line.saturating_sub(1),
                                character: u32::MAX,
                            },
                        },
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        source: Some("rivet".into()),
                        code: Some(NumberOrString::String("long-function".into())),
                        message: format!(
                            "Function `{}` is {} lines (threshold: {}). \
                             Long functions are harder to test and maintain.",
                            func.name, func.nloc, max,
                        ),
                        ..Default::default()
                    });
                }
            }
        }

        // Update state
        self.state.update_analysis(uri, result);

        // Publish diagnostics to the editor
        if let Ok(doc_uri) = uri.parse() {
            self.client.publish_diagnostics(doc_uri, diagnostics, None).await;
        }
    }
}
```

### 11.7 Server Entry Point

```rust
// rivet-lsp/src/main.rs

use tower_lsp_server::{LspService, Server};
use rivet_core::{Analyzer, AnalyzerConfig};
use crate::server::{RivetLanguageServer, LspConfig};
use crate::state::DocumentState;

mod server;
mod state;

#[tokio::main]
async fn main() {
    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let analyzer_config = AnalyzerConfig::from_file_or_default();
    let lsp_config = LspConfig {
        thresholds: analyzer_config.thresholds.clone(),
        analyze_on_change: true,
        enable_code_lenses: true,
        enable_hover: true,
    };

    let (service, socket) = LspService::new(|client| {
        RivetLanguageServer {
            client,
            analyzer: Analyzer::new(analyzer_config).expect("Failed to create analyzer"),
            state: DocumentState::new(),
            config: lsp_config,
        }
    });

    Server::new(stdin, stdout, socket).serve(service).await;
}
```

### 11.8 Editor Configuration Examples

**VS Code (`settings.json`):**

```json
{
  "rivet.enable": true,
  "rivet.analyzeOnChange": true,
  "rivet.codeLens.enable": true,
  "rivet.hover.enable": true,
  "rivet.thresholds.maxCyclomaticComplexity": 15,
  "rivet.thresholds.maxCognitiveComplexity": 15
}
```

For VS Code, a thin extension is needed to launch the LSP server. The extension's `package.json` declares the language server:

```json
{
  "contributes": {
    "configuration": {
      "title": "rivet",
      "properties": {
        "rivet.serverPath": {
          "type": "string",
          "default": "rivet-lsp",
          "description": "Path to the rivet-lsp binary"
        }
      }
    }
  }
}
```

**Neovim (with `nvim-lspconfig`):**

```lua
-- ~/.config/nvim/lua/plugins/rivet.lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

if not configs.rivet_rs then
  configs.rivet_rs = {
    default_config = {
      cmd = { 'rivet-lsp' },
      filetypes = {
        'rust', 'python', 'typescript', 'javascript', 'go', 'java',
        'c', 'cpp', 'csharp', 'ruby', 'php', 'kotlin', 'scala',
        'lua', 'swift', 'zig',
      },
      root_dir = lspconfig.util.root_pattern('rivet.toml', '.git'),
      settings = {},
    },
  }
end

lspconfig.rivet_rs.setup({})
```

**Helix (`languages.toml`):**

```toml
[[language]]
name = "rust"
language-servers = ["rust-analyzer", "rivet"]

[language-server.rivet]
command = "rivet-lsp"
```

**Zed (`settings.json`):**

```json
{
  "lsp": {
    "rivet": {
      "binary": { "path": "rivet-lsp" }
    }
  },
  "languages": {
    "Rust": { "language_servers": ["rust-analyzer", "rivet"] },
    "Python": { "language_servers": ["pyright", "rivet"] },
    "TypeScript": {
      "language_servers": ["typescript-language-server", "rivet"]
    }
  }
}
```

### 11.9 LSP Features Roadmap

| Feature                   | LSP Method                         | Priority | Description                                                 |
| :------------------------ | :--------------------------------- | :------- | :---------------------------------------------------------- |
| **Diagnostics**           | `textDocument/publishDiagnostics`  | P0       | Threshold violations as warnings/info                       |
| **Code Lenses**           | `textDocument/codeLens`            | P0       | Per-function metric summary above each function             |
| **Hover**                 | `textDocument/hover`               | P0       | Detailed metric table when hovering over a function         |
| **Code Actions**          | `textDocument/codeAction`          | P1       | "Suppress this warning" via `// rivet:ignore` comment       |
| **Workspace Diagnostics** | `workspace/diagnostic`             | P1       | Analyze all open files, report project-wide summary         |
| **Configuration**         | `workspace/didChangeConfiguration` | P1       | Dynamic threshold changes without restart                   |
| **Inlay Hints**           | `textDocument/inlayHint`           | P2       | Inline `CC:5` hints next to function signatures             |
| **Document Symbols**      | `textDocument/documentSymbol`      | P2       | Navigate by function with complexity annotations            |
| **Semantic Tokens**       | `textDocument/semanticTokens`      | P3       | Color-code functions by complexity level (green/yellow/red) |

### 11.10 LSP + MCP Coexistence

Both servers can run simultaneously. The LSP server is editor-focused (persistent, event-driven), while the MCP server is agent-focused (on-demand, request-response). They share the same `rivet-core` library and configuration:

```text
┌──────────────────────┐     ┌──────────────────────┐
│   Editor (VS Code,   │     │   AI Agent (Claude    │
│   Neovim, Zed, etc.) │     │   Code, Cursor, etc.) │
│                      │     │                        │
│   ◄── LSP client ──► │     │   ◄── MCP client ───► │
└──────────┬───────────┘     └──────────┬─────────────┘
           │ stdio/TCP                  │ stdio
    ┌──────▼──────────┐        ┌────────▼────────────┐
    │   rivet-lsp    │        │    rivet-mcp       │
    │  (tower-lsp)    │        │    (rmcp)            │
    └──────┬──────────┘        └────────┬────────────┘
           │                            │
           └──────────┬─────────────────┘
                      │
              ┌───────▼───────┐
              │  rivet-core  │
              │  (Analyzer)   │
              └───────────────┘
```

---

## 12. CLI Design

```text
rivet [COMMAND] [OPTIONS] [PATHS...]

COMMANDS:
  analyze     Analyze files and output metrics (default)
  check       Check against thresholds (exit code 1 on violations)
  languages   List supported languages
  metrics     List available metrics
  serve       Start MCP server (stdio transport)
  lsp         Start LSP server (stdio transport, for editor integration)

GLOBAL OPTIONS:
  -l, --language <LANG>       Filter to specific language(s)
  -f, --format <FORMAT>       Output format: json, sarif, csv, text [default: text]
  -o, --output <FILE>         Write output to file (default: stdout)
  -c, --config <FILE>         Config file path [default: rivet.toml]
  -j, --jobs <N>              Parallelism level [default: num_cpus]
      --plugin <WASM_FILE>    Load a WASM plugin
  -v, --verbose               Increase verbosity (-vv for debug)
  -q, --quiet                 Suppress non-error output

ANALYZE OPTIONS:
      --sort <FIELD>          Sort by: cc, cognitive, nloc, name [default: cc]
      --top <N>               Show only top N functions
      --min-cc <N>            Only show functions with CC >= N

CHECK OPTIONS:
  -C, --max-cc <N>            Max cyclomatic complexity [default: 15]
      --max-cognitive <N>     Max cognitive complexity [default: 15]
  -a, --max-params <N>        Max parameter count [default: 5]
      --max-length <N>        Max function length [default: 100]
      --max-nesting <N>       Max nesting depth [default: 5]
      --warning-only           Exit 0 even on violations (only warn)

LSP OPTIONS:
      --tcp <PORT>            Use TCP transport instead of stdio
      --analyze-on-change     Re-analyze on every keystroke [default: true]
      --no-code-lens          Disable code lens annotations
      --no-hover              Disable hover information

EXAMPLES:
  rivet analyze ./src
  rivet check ./src --max-cc 10 --format sarif -o results.sarif
  rivet analyze ./src -l python -l rust --top 20 --sort cognitive
  rivet serve  # Start MCP server on stdio
  rivet lsp    # Start LSP server on stdio (for editor integration)
  rivet lsp --tcp 9257  # Start LSP server on TCP port
```

---

## 13. Output Formats

### 13.1 JSON Output

Directly serializes the `ProjectAnalysis` / `FileAnalysis` structs via serde. This is the canonical programmatic output.

### 13.2 SARIF v2.1.0 Output

```rust
// rivet-core/src/output/sarif.rs

pub fn to_sarif(analysis: &ProjectAnalysis, config: &SarifConfig) -> SarifLog {
    SarifLog {
        version: "2.1.0".into(),
        schema: "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json".into(),
        runs: vec![SarifRun {
            tool: SarifTool {
                driver: SarifToolComponent {
                    name: "rivet".into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    information_uri: "https://github.com/user/rivet".into(),
                    rules: generate_rules(&analysis),  // One rule per threshold
                },
            },
            results: generate_results(&analysis),  // One result per violation
        }],
    }
}
```

Each threshold violation becomes a SARIF `result` with precise location, severity, and remediation guidance.

### 13.3 CSV Output

Simple tabular format compatible with spreadsheet tools and pandas:

```csv
file,function,start_line,end_line,cc,cognitive,params,nloc,tokens
src/main.rs,main,1,50,3,2,0,48,120
src/parser.rs,parse,10,80,12,15,3,65,280
```

### 13.4 Human-Readable Text Output

```text
src/parser.rs
  parse               CC=12  Cognitive=15  Params=3  NLOC=65  Tokens=280
  tokenize            CC=8   Cognitive=10  Params=2  NLOC=45  Tokens=180

src/main.rs
  main                CC=3   Cognitive=2   Params=0  NLOC=48  Tokens=120

Summary: 3 functions in 2 files | Avg CC: 7.7 | Total NLOC: 158
Violations: 1 function exceeds CC threshold (15)
```

---

## 14. Configuration System

### 14.1 `rivet.toml` Configuration File

```toml
# rivet.toml — Project-level configuration

[analysis]
# Languages to analyze (empty = auto-detect all)
languages = []
# File patterns to include
include = ["**/*.rs", "**/*.py", "**/*.ts"]
# File patterns to exclude
exclude = ["**/vendor/**", "**/node_modules/**", "**/*.generated.*"]
# Number of parallel jobs (0 = auto)
jobs = 0

[thresholds]
max_cyclomatic_complexity = 15
max_cognitive_complexity = 15
max_function_length = 100
max_parameter_count = 5
max_nesting_depth = 5
min_maintainability_index = 20.0

# Override thresholds for specific paths
[[thresholds.overrides]]
paths = ["**/tests/**", "**/test_*"]
max_cyclomatic_complexity = 25  # Tests can be more complex
max_function_length = 200

[output]
format = "text"            # text, json, sarif, csv
sort_by = "cc"             # cc, cognitive, nloc, name
show_top = 0               # 0 = show all

[plugins]
paths = [".rivet/plugins/"]
# Individual plugin entries
[[plugins.entries]]
name = "my-custom-metric"
path = ".rivet/plugins/my_metric.wasm"
config = { some_option = "value" }

[lsp]
# Analyze on every keystroke vs only on save
analyze_on_change = true
# Debounce interval for didChange events (milliseconds)
debounce_ms = 300
# Show code lenses with per-function metrics
enable_code_lenses = true
# Show hover information with metric details
enable_hover = true
# Diagnostic severity: "warning" or "information"
diagnostic_severity = "warning"
```

### 14.2 Configuration Resolution Order

1. CLI flags (highest priority)
2. Environment variables (`RIVET_MAX_CC=10`)
3. Project `rivet.toml` (found by walking up from CWD)
4. User config `~/.config/rivet/config.toml`
5. Built-in defaults (lowest priority)

---

## 15. Testing Strategy

### 15.1 Unit Tests

Every metric implementation has unit tests with known source code inputs and expected outputs.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cyclomatic_simple_if() {
        let source = b"fn foo(x: i32) -> i32 { if x > 0 { x } else { -x } }";
        let result = analyze_function(source, Language::Rust);
        assert_eq!(result.cyclomatic_complexity, 2);  // 1 base + 1 if
    }

    #[test]
    fn test_cyclomatic_nested_control_flow() {
        let source = b"fn bar(x: i32, y: i32) -> i32 {
            if x > 0 {
                if y > 0 { x + y }
                else { x - y }
            } else {
                match x {
                    0 => 0,
                    _ => -x,
                }
            }
        }";
        let result = analyze_function(source, Language::Rust);
        assert_eq!(result.cyclomatic_complexity, 5);
        // 1 base + 1 outer if + 1 inner if + 2 match arms
    }
}
```

### 15.2 Snapshot Tests (insta)

For complex outputs (SARIF, JSON), use `insta` snapshot testing. This is particularly AI-agent-friendly: an agent can run `cargo insta review` to accept/reject changes.

```rust
#[test]
fn test_sarif_output_python_example() {
    let source = include_str!("../../tests/fixtures/python/complex.py");
    let result = analyzer.analyze_source(source.as_bytes(), Language::Python, None).unwrap();
    let sarif = to_sarif(&result.into(), &SarifConfig::default());
    insta::assert_json_snapshot!(sarif);
}
```

### 15.3 Cross-Language Accuracy Tests

Compare rivet output against known-good values from Lizard and rust-code-analysis for the same source files:

```text
tests/fixtures/
├── python/
│   ├── simple.py
│   ├── simple.expected.json       # Expected metrics
│   ├── complex.py
│   └── complex.expected.json
├── rust/
│   ├── simple.rs
│   └── simple.expected.json
├── typescript/
│   ├── simple.ts
│   └── simple.expected.json
└── ...
```

### 15.4 Property-Based Tests

Use `proptest` for fuzz-like testing of parser robustness:

```rust
proptest! {
    #[test]
    fn parser_never_panics(source in "\\PC{0,10000}") {
        let _ = parser.parse(source.as_bytes(), &rust_config);
        // Should never panic, even on garbage input
    }
}
```

### 15.5 Integration Tests for Bindings

Python:

```python
# tests/test_python_bindings.py
import rivet_rs

def test_analyze_python():
    analyzer = rivet_rs.Analyzer()
    result = analyzer.analyze_source(
        "def foo(x):\n  if x > 0:\n    return x\n  return -x",
        "python",
    )
    assert len(result.functions) == 1
    assert result.functions[0].name == "foo"
    assert result.functions[0].cyclomatic_complexity == 2

def test_threshold_violation():
    analyzer = rivet_rs.Analyzer(config={"thresholds": {"max_cyclomatic_complexity": 1}})
    result = analyzer.analyze_source("def foo(x):\n  if x: return 1\n  return 0", "python")
    project = analyzer.to_project([result])
    violations = analyzer.check_thresholds(project)
    assert violations.has_violations
```

TypeScript:

```typescript
// tests/test_node_bindings.test.ts
import { Analyzer } from "@rivet-rs/node";

test("analyze TypeScript source", () => {
  const analyzer = new Analyzer();
  const result = analyzer.analyzeSource(
    `function foo(x: number): number {
      if (x > 0) return x;
      return -x;
    }`,
    "typescript",
  );
  expect(result.functions).toHaveLength(1);
  expect(result.functions[0].cyclomaticComplexity).toBe(2);
});
```

### 15.6 LSP Server Tests

The LSP server is tested at two levels:

**Unit tests** verify diagnostic generation logic in isolation:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostic_generation_high_cc() {
        let analyzer = Analyzer::new(AnalyzerConfig::default()).unwrap();
        let source = include_str!("../../tests/fixtures/rust/high_cc.rs");
        let result = analyzer.analyze_source(source.as_bytes(), Language::Rust, None).unwrap();

        let thresholds = Thresholds { max_cyclomatic_complexity: Some(5), ..Default::default() };
        let diagnostics = generate_diagnostics(&result, &thresholds);

        assert!(!diagnostics.is_empty());
        assert!(diagnostics.iter().any(|d| d.code == Some(
            NumberOrString::String("high-cyclomatic-complexity".into())
        )));
    }

    #[test]
    fn test_code_lens_generation() {
        let analyzer = Analyzer::new(AnalyzerConfig::default()).unwrap();
        let source = b"fn foo() {} fn bar(x: i32) { if x > 0 {} }";
        let result = analyzer.analyze_source(source, Language::Rust, None).unwrap();

        let lenses = generate_code_lenses(&result);
        assert_eq!(lenses.len(), 2);  // One per function
        assert!(lenses[0].command.as_ref().unwrap().title.contains("CC:"));
    }
}
```

**Integration tests** verify the full LSP protocol flow using a test harness:

```rust
#[tokio::test]
async fn test_lsp_open_document_publishes_diagnostics() {
    let (service, socket) = LspService::new(|client| {
        RivetLanguageServer::new_for_testing(client)
    });

    // Simulate editor opening a file with high complexity
    let open_params = DidOpenTextDocumentParams {
        text_document: TextDocumentItem {
            uri: "file:///test.rs".parse().unwrap(),
            language_id: "rust".into(),
            version: 0,
            text: include_str!("../../tests/fixtures/rust/high_cc.rs").into(),
        },
    };

    // Send didOpen, verify diagnostics are published
    // (uses tower-lsp-server test utilities)
}
```

---

## 16. Performance Engineering

### 16.1 Parallelism Model

```text
┌─────────────┐
│  File Walker │  (single thread, produces work items)
│   (ignore)   │
└──────┬──────┘
       │ Channel<FileInput>
       ▼
┌──────────────────────────────┐
│     rayon Thread Pool        │
│  ┌───────┐ ┌───────┐        │
│  │Parse  │ │Parse  │ ...    │
│  │+Analyze│ │+Analyze│       │
│  │File 1 │ │File 2 │        │
│  └───────┘ └───────┘        │
└──────────────┬───────────────┘
               │ Vec<FileAnalysis>
               ▼
┌──────────────────────────────┐
│  Aggregation + Output        │  (single thread)
│  (ProjectAnalysis → format)  │
└──────────────────────────────┘
```

Each rayon thread has its own `thread_local!` tree-sitter `Parser` instance (because `Parser` is `!Send`). The parsed `Tree` objects are `Send` and can be aggregated.

### 16.2 Performance Targets

| Metric                             | Target  | Rationale                                       |
| :--------------------------------- | :------ | :---------------------------------------------- |
| Single file (1K LOC)               | < 10ms  | Interactive feedback in agent loops             |
| Single file (10K LOC)              | < 100ms | Acceptable for CI                               |
| 1000 files parallel                | < 5s    | Monorepo scale on modern hardware               |
| Memory per file                    | < 10MB  | Bounded by tree-sitter tree size                |
| Cold start (CLI)                   | < 200ms | Negligible overhead in CI                       |
| MCP server startup                 | < 100ms | Near-instant for agent tool calls               |
| LSP server startup                 | < 150ms | Editor should feel instant                      |
| LSP diagnostic latency (on save)   | < 200ms | Perceived real-time for save-triggered analysis |
| LSP diagnostic latency (on change) | < 500ms | Debounced; must not lag the editor              |
| LSP code lens response             | < 100ms | Must not block editor rendering                 |
| LSP hover response                 | < 50ms  | Must feel instant on hover                      |

### 16.3 Optimization Techniques

1. **tree-sitter query compilation**: Queries are compiled once at `Analyzer` construction and reused for every file.
2. **Arena allocation**: Use `bumpalo` for temporary AST traversal allocations.
3. **Incremental analysis** (future): Cache file hashes and skip re-analysis of unchanged files.
4. **Streaming output**: For large projects, output results as they complete (not waiting for all files).
5. **Feature-gated grammars**: Only compile grammars the user needs.
6. **LSP debouncing**: The LSP server debounces `didChange` events (configurable, default 300ms) to avoid re-analyzing on every keystroke. Only the latest document version is analyzed.
7. **LSP analysis cancellation**: If a new `didChange` arrives while analysis is in-flight, the previous analysis is abandoned (cooperative cancellation via `tokio::select!`).
8. **LSP cached results**: Code lens and hover requests read from cached `DocumentEntry.analysis` — they never trigger re-analysis, ensuring sub-50ms response times.

---

## 17. Security Model

### 17.1 Plugin Sandboxing

WASM plugins run in an Extism/Wasmtime sandbox with:

- **No filesystem access**: Plugins cannot read or write files.
- **No network access**: Plugins cannot make HTTP requests.
- **Memory limits**: Configurable maximum memory (default 16MB).
- **CPU time limits**: Configurable timeout per plugin call (default 5s).
- **No ambient authority**: Plugins only receive the data explicitly passed to them.

### 17.2 Input Validation

- Source code input is treated as untrusted byte slices.
- File paths are canonicalized and checked against configured include/exclude patterns.
- WASM plugin bytecodes are validated by Wasmtime before execution.
- No shell commands are ever executed by the core library.

### 17.3 Supply Chain

- Minimal dependency tree for the core library.
- `cargo-deny` configured to reject known-vulnerable crates.
- All tree-sitter grammar crates are from the official `tree-sitter` GitHub org.
- WASM plugins are loaded from user-configured paths only (no auto-download).

---

## 18. Build System and CI/CD

### 18.1 Workspace Cargo.toml

```toml
[workspace]
resolver = "2"
members = [
    "crates/rivet-core",
    "crates/rivet-cli",
    "crates/rivet-mcp",
    "crates/rivet-lsp",
    "crates/rivet-python",
    "crates/rivet-node",
    "crates/rivet-plugin-sdk",
]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
repository = "https://github.com/user/rivet"

[workspace.dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
thiserror = "2"
tree-sitter = "0.25"
rayon = "1.10"
```

### 18.2 CI/CD Pipeline (GitHub Actions)

```yaml
name: CI
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo test --workspace
      - run: cargo clippy --workspace -- -D warnings
      - run: cargo fmt --check

  python-bindings:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/setup-python@v5
        with: { python-version: "3.12" }
      - run: pip install maturin pytest
      - run: cd crates/rivet-python && maturin develop --release
      - run: pytest tests/python/

  node-bindings:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: actions/setup-node@v4
        with: { node-version: "22" }
      - run: cd crates/rivet-node && npm install && npm run build
      - run: npm test

  release:
    if: startsWith(github.ref, 'refs/tags/v')
    needs: [test, python-bindings, node-bindings]
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build CLI binaries
        run: cargo build --release -p rivet-cli -p rivet-mcp -p rivet-lsp
      - name: Build Python wheels
        uses: PyO3/maturin-action@v1
        with:
          command: build
          args: --release --strip -m crates/rivet-python/Cargo.toml
      - name: Publish to crates.io
        run: cargo publish -p rivet-core && cargo publish -p rivet-cli
      - name: Publish to PyPI
        run: maturin publish -m crates/rivet-python/Cargo.toml
      - name: Publish to npm
        run: cd crates/rivet-node && npm publish
```

---

## 19. Agent-Driven Development Strategy

Since this project will be implemented using AI coding agents (Claude Code, Codex, Cursor, Gemini CLI), the design must be agent-friendly:

### 19.1 Principles for Agent-Driven Implementation

1. **Small, isolated crates**: Each crate has a focused responsibility. An agent can work on `rivet-core/src/metrics/cyclomatic.rs` without needing context about the CLI or MCP server.

2. **Comprehensive `CLAUDE.md`**: The repository root contains agent instructions:

```markdown
# CLAUDE.md

## Project: rivet

### Build

cargo build --workspace
cargo test --workspace

### Architecture

- `rivet-core`: Pure Rust library. No IO. Core types in `src/types.rs`.
- `rivet-cli`: CLI binary using clap v4.
- `rivet-mcp`: MCP server using rmcp.
- `rivet-lsp`: LSP server using tower-lsp-server.
- `rivet-python`: PyO3 bindings.
- `rivet-node`: napi-rs bindings.

### Conventions

- All public types derive `Debug, Clone, Serialize, Deserialize`.
- Use `thiserror` for error types.
- Use `insta` for snapshot tests.
- Never use `unwrap()` in library code.
- Metric implementations must implement `MetricAnalyzer` trait.

### Testing

cargo test --workspace # All tests
cargo test -p rivet-core # Core only
cargo insta review # Review snapshot changes

### Adding a new metric

1. Create `crates/rivet-core/src/metrics/<name>.rs`
2. Implement `MetricAnalyzer` trait
3. Register in `MetricRegistry::with_defaults()`
4. Add tests with known inputs
5. Update snapshot tests
```

1. **Snapshot tests everywhere**: Agents can modify code and verify correctness by running `cargo test`. Snapshot diffs provide clear feedback on what changed.

2. **Feature isolation**: Each language grammar is a separate feature flag. An agent adding Kotlin support need only add the feature and query files — no existing code is modified.

3. **Trait-based extensibility**: Adding a new metric or output format requires implementing a trait in a new file and registering it — no modification of existing code paths.

### 19.2 Recommended Agent Task Decomposition

| Task                               | Agent       | Scope                                  | Verification                             |
| :--------------------------------- | :---------- | :------------------------------------- | :--------------------------------------- |
| Scaffold workspace + Cargo.toml    | Claude Code | Workspace root                         | `cargo build --workspace` compiles       |
| Implement tree-sitter parser layer | Claude Code | `rivet-core/src/parser/`               | Unit tests pass                          |
| Write Rust function queries        | Cursor      | `queries/rust/*.scm`                   | Snapshot tests match expected            |
| Implement cyclomatic complexity    | Claude Code | `rivet-core/src/metrics/cyclomatic.rs` | Unit tests against known CC values       |
| Implement cognitive complexity     | Claude Code | `rivet-core/src/metrics/cognitive.rs`  | Unit tests against SonarSource examples  |
| Implement Halstead metrics         | Codex       | `rivet-core/src/metrics/halstead.rs`   | Cross-validation with rust-code-analysis |
| Implement LOC metrics              | Gemini CLI  | `rivet-core/src/metrics/loc.rs`        | Fixture files with expected counts       |
| Implement CLI                      | Claude Code | `rivet-cli/`                           | `cargo run -- analyze tests/fixtures/`   |
| Implement MCP server               | Claude Code | `rivet-mcp/`                           | Manual test with Claude Desktop          |
| Implement LSP server core          | Claude Code | `rivet-lsp/`                           | `cargo build -p rivet-lsp` compiles      |
| Implement LSP diagnostics          | Claude Code | `rivet-lsp/src/server.rs`              | Open file in Neovim → warnings appear    |
| Implement LSP code lenses          | Cursor      | `rivet-lsp/src/server.rs`              | Metrics shown above each function        |
| Implement LSP hover                | Cursor      | `rivet-lsp/src/server.rs`              | Hover on function → metric table appears |
| Create VS Code extension stub      | Claude Code | `editors/vscode/`                      | Extension activates and launches LSP     |
| Implement PyO3 bindings            | Claude Code | `rivet-python/`                        | `pytest` passes                          |
| Implement napi-rs bindings         | Claude Code | `rivet-node/`                          | `npm test` passes                        |
| Implement SARIF output             | Claude Code | `rivet-core/src/output/sarif.rs`       | SARIF validator passes                   |
| Implement Extism plugin host       | Claude Code | `rivet-core/src/plugin/`               | Example plugin loads and runs            |
| Write Python query files           | Cursor      | `queries/python/*.scm`                 | Snapshot tests                           |
| Write TypeScript query files       | Cursor      | `queries/typescript/*.scm`             | Snapshot tests                           |
| Write Go query files               | Cursor      | `queries/go/*.scm`                     | Snapshot tests                           |
| Write Java query files             | Cursor      | `queries/java/*.scm`                   | Snapshot tests                           |

### 19.3 Agent Skill Files

Each crate can have its own `AGENTS.md` or `.cursor/rules/*.md` for agent-specific context:

```markdown
# crates/rivet-core/AGENTS.md

## Context

This is the pure Rust core library. It has NO IO, NO async, NO CLI dependencies.
All public functions take &[u8] source code and return structured Result types.

## Key files

- src/types.rs: All data types (FileAnalysis, FunctionAnalysis, etc.)
- src/metrics/trait.rs: MetricAnalyzer trait definition
- src/parser/mod.rs: tree-sitter wrapper
- src/language.rs: Language enum and registry

## Testing

cargo test -p rivet-core

## Constraints

- Never add tokio, clap, or any IO dependency here
- All errors use RivetError from src/error.rs
- All public types must derive Serialize, Deserialize
```

```markdown
# crates/rivet-lsp/AGENTS.md

## Context

LSP server binary. Provides real-time complexity diagnostics in editors.
Uses tower-lsp-server (community fork) for the LSP protocol layer.
All analysis logic is delegated to rivet-core::Analyzer — this crate
only handles document state management, diagnostic conversion, and
code lens/hover generation.

## Key files

- src/main.rs: Server entry point (stdio transport setup)
- src/server.rs: LanguageServer trait implementation
- src/state.rs: DashMap-based document state management

## Testing

cargo test -p rivet-lsp

# Manual: open a .rs file in Neovim with rivet LSP configured

## Constraints

- Never import rivet-cli or rivet-mcp
- All analysis goes through self.analyzer (rivet-core)
- Diagnostic messages must include actionable guidance
- Code lens and hover read from cached state, never trigger re-analysis
- Debounce didChange events to avoid CPU spikes
```

---

## 20. Phased Implementation Plan

### Phase 1: Foundation (Weeks 1–3)

**Goal:** Working Rust core with CC + LOC metrics for Rust and Python.

- [ ] Scaffold Cargo workspace with all crate stubs
- [ ] Implement `LanguageRegistry` with Rust + Python grammars
- [ ] Write tree-sitter query files for Rust and Python (functions, control flow)
- [ ] Implement `CyclomaticComplexity` metric
- [ ] Implement `LinesOfCode` metric (PLOC, SLOC, CLOC, BLANK)
- [ ] Implement `ParameterCount` metric
- [ ] Implement JSON output formatter
- [ ] Implement basic CLI (`analyze` command)
- [ ] Set up CI with `cargo test`, `clippy`, `fmt`
- [ ] Create test fixtures with expected values

**Deliverable:** `rivet analyze ./src --format json` works for Rust and Python files.

### Phase 2: Full Metrics + More Languages (Weeks 4–6)

**Goal:** Complete metrics suite, 10+ languages.

- [ ] Implement `CognitiveComplexity` metric
- [ ] Implement `HalsteadMetrics` metric
- [ ] Implement `MaintainabilityIndex` metric
- [ ] Implement `NestingDepth` metric
- [ ] Add tree-sitter queries for TypeScript, JavaScript, Go, Java, C, C++, C#, Ruby, PHP, Kotlin
- [ ] Implement `check` command with threshold enforcement
- [ ] Implement text (human-readable) output format
- [ ] Implement CSV output format
- [ ] Implement `--sort`, `--top`, `--min-cc` filtering options
- [ ] Set up snapshot tests for all metrics × all languages

**Deliverable:** Full metric suite working for 12 languages with `check` command.

### Phase 3: Language Bindings (Weeks 7–9)

**Goal:** Python and Node.js bindings published.

- [ ] Implement PyO3 bindings in `rivet-python`
- [ ] Generate Python type stubs (`.pyi`)
- [ ] Write Python integration tests
- [ ] Set up maturin build pipeline
- [ ] Implement napi-rs bindings in `rivet-node`
- [ ] Verify auto-generated TypeScript definitions
- [ ] Write Node.js integration tests
- [ ] Set up npm build pipeline
- [ ] Publish to PyPI (test) and npm (test)

**Deliverable:** `pip install rivet-rs` and `npm install @rivet-rs/node` work.

### Phase 4: AI Agent + IDE Integration (Weeks 10–13)

**Goal:** MCP server, LSP server, and SARIF output operational.

- [ ] Implement SARIF v2.1.0 output formatter
- [ ] Validate SARIF output with SARIF validator
- [ ] Implement MCP server with `rmcp`
- [ ] Test MCP server with Claude Desktop and Cursor
- [ ] Implement LSP server with `tower-lsp-server`
- [ ] Implement LSP diagnostic publishing (threshold violations)
- [ ] Implement LSP code lenses (per-function metric annotations)
- [ ] Implement LSP hover (detailed metric tables)
- [ ] Test LSP server with Neovim (`nvim-lspconfig`) and Helix
- [ ] Create minimal VS Code extension for LSP client
- [ ] Create GitHub Action for complexity checking
- [ ] Write `CLAUDE.md`, `AGENTS.md`, `.cursor/rules/` files
- [ ] Write editor configuration documentation (VS Code, Neovim, Helix, Zed)
- [ ] Write documentation for AI agent integration patterns

**Deliverable:** AI agents can invoke rivet via MCP; editors show real-time diagnostics via LSP; SARIF uploads to GitHub Code Scanning.

### Phase 5: Plugin System (Weeks 14–16)

**Goal:** WASM plugin system with SDK and example plugin.

- [ ] Implement Extism plugin host in `rivet-core`
- [ ] Define plugin interface contract (JSON schema)
- [ ] Create `rivet-plugin-sdk` crate
- [ ] Build example plugin (e.g., "function name length" metric)
- [ ] Implement plugin discovery from filesystem
- [ ] Add `rivet.toml` plugin configuration
- [ ] Write plugin development documentation
- [ ] Test plugin sandboxing (memory limits, timeout)

**Deliverable:** Users can write and load custom metric plugins.

### Phase 6: Scale and Polish (Weeks 17+)

**Goal:** Production readiness.

- [ ] Add remaining languages to reach 170+ (via tree-sitter-language-pack)
- [ ] Implement incremental analysis with file hash caching
- [ ] Performance benchmarking and optimization
- [ ] Implement `rivet.toml` path-based threshold overrides
- [ ] Implement LSP code actions ("suppress warning" via `// rivet:ignore`)
- [ ] Implement LSP inlay hints (inline `CC:5` next to function signatures)
- [ ] Implement LSP `workspace/didChangeConfiguration` for dynamic threshold updates
- [ ] Implement LSP document symbols with complexity annotations
- [ ] Publish CLI binaries via GitHub Releases
- [ ] Publish to crates.io, PyPI, npm
- [ ] Publish VS Code extension to VS Code Marketplace
- [ ] Write comprehensive documentation site
- [ ] Cross-validate metrics against Lizard and rust-code-analysis outputs

---

## 21. Open Questions and Future Work

### Open Questions

| #   | Question                                                                                       | Impact                                                | Decision Needed By |
| :-- | :--------------------------------------------------------------------------------------------- | :---------------------------------------------------- | :----------------- |
| Q1  | Should we depend on `tree-sitter-language-pack` or vendor individual grammars?                 | Build complexity vs. maintenance                      | Phase 1            |
| Q2  | Should the MCP server support SSE transport in addition to stdio?                              | Remote agent deployments                              | Phase 4            |
| Q3  | Should the LSP and MCP servers share a single process (multiplex) or remain separate binaries? | Resource usage vs. simplicity                         | Phase 4            |
| Q4  | Should the LSP server debounce `didChange` events to avoid re-analyzing on every keystroke?    | Latency vs. CPU usage; configurable debounce interval | Phase 4            |
| Q5  | Should plugin interface include raw AST node access or only S-expressions?                     | Plugin power vs. simplicity                           | Phase 5            |
| Q6  | License: MIT, Apache-2.0, or dual?                                                             | Community adoption                                    | Phase 1            |
| Q7  | Should the VS Code extension embed the LSP binary or require separate installation?            | Distribution simplicity vs. binary size               | Phase 6            |

### Future Work

- **Differential Analysis**: Compare complexity between two commits/branches.
- **AI-Enriched SARIF**: Embed refactoring suggestions in SARIF output using LLM analysis.
- **Browser WASM Build**: Compile core to WASM for in-browser code analysis.
- **dbt Semantic Layer Integration**: Use complexity metrics as data quality signals in dbt pipelines.
- **Terraform Provider**: Expose complexity thresholds as Terraform-managed infrastructure (governance as IaC).
- **LSP Semantic Tokens**: Color-code functions by complexity level (green/yellow/red) directly in editor syntax highlighting.
- **LSP Notebook Support**: Provide complexity metrics inside Jupyter/Marimo notebook cells.
- **Unified Server Binary**: Single `rivet-server` binary that speaks both LSP and MCP, selected by transport negotiation.
