# Rust Workspace Template - Claude Code Memory

## Project Overview

This repository is a production-ready Rust workspace template.

Codex-specific project guidance lives in `AGENTS.md`. Keep Claude-only workflow details here and under `.claude/`.
Codex sandbox defaults for this repository are checked in at `.codex/config.toml`.

- **Build System**: Cargo workspace
- **Linting/Formatting**: Clippy, rustfmt, and Trunk
- **Testing**: `cargo test --workspace --all-features`
- **Security**: GitHub CodeQL and Trunk security linters

## Quick Commands

```bash
make setup      # Fetch Cargo dependencies
make lint       # Run Trunk plus strict workspace clippy
make format     # Format Rust and repo files
make test       # Run workspace tests
make codeql     # Run local CodeQL analysis
make build      # Build release binaries and libraries
make clean      # Remove build artifacts
```

## Rust Guardrails

- Prefer shared versions in `[workspace.dependencies]` over duplicating dependency versions in member crates.
- Each crate must opt into workspace lints with:

```toml
[lints]
workspace = true
```

- Keep `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean.
- Treat Clippy `pedantic`, `cargo`, and `cognitive_complexity` findings as mandatory fixes.
- Refactor functions before they become hard to read; the cognitive complexity threshold is `10`.
- Avoid `unsafe` unless there is a documented need and explicit review.

## Testing

- **Unit Tests**: Place in the same file as the code or in a `tests` module within the crate.
- **Snapshot Tests**: Use `insta` for large structured outputs (JSON, SARIF) or complex AST queries.
- **Integration Tests**: Place under `crates/<crate-name>/tests/`.
- **Benchmark**: Use `criterion` for performance-critical analysis paths.
- **Command**: `cargo test --workspace`

## Architecture

- **Root `Cargo.toml`**: Defines the workspace and shared dependency versions.
- **`crates/rivet-core`**: Pure Rust library. No IO, no async, no CLI dependencies. Core entry points take `&[u8]` and return structured results.
- **`crates/rivet-cli`**: Primary CLI application (clap v4). Owns filesystem IO and output formatting.
- **`crates/rivet-mcp`**: Model Context Protocol server (rmcp). Enables AI agent integration.
- **`crates/rivet-lsp`**: Language Server Protocol server (tower-lsp-server). Real-time IDE integration.
- **`crates/rivet-python` & `crates/rivet-node`**: Native bindings (PyO3, napi-rs).
- **`queries/`**: Tree-sitter query files (`.scm`) for language-specific pattern matching.
- **`dev/`**: Helper scripts for setup, lint, build, test, and CodeQL flows.

## Core Philosophy: Functional Core, Imperative Shell

1. **`rivet-core` is pure**: It handles parsing (via tree-sitter) and metric computation. It must never perform filesystem IO or network requests.
2. **Consumers own IO**: The CLI, MCP, and LSP servers are responsible for reading files, managing concurrency (rayon/tokio), and handling transport.
3. **Declarative Languages**: Add support for new languages by adding Tree-sitter queries in `queries/<lang>/*.scm`, not by adding Rust code.
4. **WASM Plugins**: Use Extism for safe, language-agnostic extensibility.

## Common Gotchas

- Do not duplicate dependency versions inside member crates when the dependency can live in `[workspace.dependencies]`.
- Keep `Cargo.lock` committed for this template because it includes an executable crate.
- Trunk manages non-Rust repo linters hermetically; do not replace it with ad hoc local installs.
- If a new member crate is added, update workspace membership and ensure it enables workspace lints.

## Git Workflow

- Create feature branches from `main`.
- Use conventional commit messages such as `feat(cli): add init command`.
- Run `make lint && make test` before commits.
- Record release notes with the `manage-changelog` skill when that workflow is in use.

## Available Skills

- `initialize-project`: rename the template and its workspace members
- `manage-adr`: maintain architecture decisions in `docs/adr`
- `manage-changelog`: maintain changelog fragments when enabled
- `.claude/skills` remains the canonical skill source even when other agents consume the mirrored tree under `.agents/skills`

## Self-Improvement & Autonomy

- **Proactive Documentation**: Claude is authorized to autonomously refine project rules, `CLAUDE.md`, and `AGENTS.md` when new patterns, conventions, or improvements are identified.
- **Autonomous ADRs**: When making significant architectural decisions or choosing between technical approaches, Claude should proactively use the `manage-adr` skill to document the "why". Do not wait for user prompts to record finalized designs.
- **Skill Evolution**: Prefer contributing to or refining reusable skills under `.claude/skills/` to ensure process improvements survive across sessions.
- **Refinement Loop**: Add or refine rules in this file whenever recurring mistakes or sub-optimal patterns are observed.
