# 5. Query-based Language Behavior Definition

Date: 2026-03-17

## Status

Accepted

Depends on [2. Universal Parsing with Tree-sitter](0002-universal-parsing-with-tree-sitter.md)

## Context

To compute metrics like cyclomatic complexity, cognitive complexity, and Halstead metrics, Rivet must identify specific patterns in the source code's AST:

- **Functions/Methods**: Where does a function start and end?
- **Control Flow**: What are the branching points (if, for, while, match)?
- **Operators/Operands**: What counts as an operator (for Halstead metrics)?

Every programming language names these nodes differently (e.g., `function_item` in Rust vs. `function_definition` in Python). Hard-coding these mappings in Rust source code for 170+ languages would create a massive maintenance burden and make the core logic cluttered with language-specific details.

## Decision

We will use **Tree-sitter Query Files (`.scm`)** to define language-specific behavior declaratively.

- **Standardized Tags**: The Rust engine will search for standardized "tags" produced by these queries (e.g., `@function.def`, `@cc.branch`, `@op.math`).
- **Separation of Concerns**: The Rust metrics engine will operate on these tags generically, while the `.scm` files handle the mapping from language-specific AST nodes to those tags.
- **Query Registry**: Each language in the `LanguageRegistry` will bundle its own directory of query files:
  - `functions.scm`
  - `control_flow.scm`
  - `operators.scm`
  - `operands.scm`

## Consequences

### Positive

- **Declarative Extension**: Adding a new language often requires zero changes to the Rust core — only new `.scm` files.
- **Maintainability**: Language-specific logic is isolated and easier to audit than procedural code.
- **Flexibility**: Complex patterns (like identifying closures assigned to variables) can be expressed concisely in the query DSL.
- **Performance**: Tree-sitter queries are compiled and optimized, providing fast matching.

### Negative / Risks

- **DSL Learning Curve**: Developers must learn the Tree-sitter query S-expression syntax.
- **Grammar Coupling**: Queries are tightly coupled to the structure of a specific grammar. If a grammar is updated, the queries may need corresponding updates.
- **Validation**: It is harder to unit-test declarative queries than Rust code (mitigated by snapshot testing).
