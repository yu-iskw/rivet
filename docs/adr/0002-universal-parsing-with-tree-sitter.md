# 2. Universal Parsing with Tree-sitter

Date: 2026-03-17

## Status

Accepted

Prerequisite for [5. Query-based Language Behavior Definition](0005-query-based-language-behavior-definition.md)

## Context

Rivet aims to be a governance-grade code complexity analyzer supporting over 170 programming languages. Traditional approaches to multi-language parsing often involve:

1. **Regex-based matching** (e.g., Lizard): Fast but imprecise and difficult to maintain for complex language constructs.
2. **Language-specific compiler front-ends**: Highly accurate but extremely heavy and impossible to scale to 170+ languages within a single tool.
3. **Hand-written parsers**: High maintenance burden and error-prone.

We need a solution that provides high performance, error tolerance (essential for IDE/LSP use cases), and a unified interface for many languages.

## Decision

We will use **Tree-sitter** as the core parsing engine for Rivet.

Tree-sitter is an incremental parsing system that builds and maintains concrete syntax trees for source files. It is designed to be fast enough to parse on every keystroke in a text editor and robust enough to provide useful results even in the presence of syntax errors.

Key implementation details:

- **Pure Rust Core**: `rivet-core` will wrap the Tree-sitter C library.
- **Declarative Behavior**: Language-specific logic will be defined via Tree-sitter queries (`.scm` files) rather than procedural code.
- **Feature Gating**: To manage build times and binary size, language grammars will be feature-gated (e.g., `lang-rust`, `lang-python`).

## Consequences

### Positive

- **Uniform AST**: Provides a consistent way to traverse code across vastly different languages.
- **Robustness**: Gracefully handles syntax errors, allowing metrics to be computed on "broken" code.
- **Performance**: Extremely fast parsing (often sub-millisecond) and support for incremental updates.
- **Ecosystem**: Leverages a huge library of existing, high-quality grammars used by major editors (Atom, VS Code, Neovim).

### Negative / Risks

- **Build Complexity**: Requires a C/C++ compiler and `cc` crate to build the grammars.
- **Binary Size**: Including many grammars significantly increases binary size (mitigated by feature flags).
- **WASM Compatibility**: Requires careful management of Tree-sitter's C state when targeting WASM (mitigated by using Extism for plugins).
