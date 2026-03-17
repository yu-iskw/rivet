# Rivet

Rivet is an AI-agent-native code complexity analyzer for Rust and other languages.
It provides a pure Rust analysis core, a CLI, protocol integrations, and binding/plugin
surfaces that can be expanded over time.

## AI Assistants

- Codex should use [AGENTS.md](./AGENTS.md) for repo-specific instructions and verification expectations.
- Codex sandbox defaults are checked in at `.codex/config.toml` so trusted clones share the same project baseline.
- Local Codex settings may still override or extend user behavior, but this repository config is the intended default for work in this repo.
- Use `AGENTS.md` and `.codex/config.toml` together as the source of truth for Codex-specific behavior.
- Claude-specific workflow details remain in `CLAUDE.md` and `.claude/`.
- Shared reusable skills are authored in `.claude/skills` and exposed to other agents through the symlink mirror in `.agents/skills`.

## Workspace Layout

```text
.
├── Cargo.toml
├── crates/
│   ├── rivet-cli/
│   ├── rivet-core/
│   ├── rivet-lsp/
│   ├── rivet-mcp/
│   ├── rivet-node/
│   ├── rivet-plugin-sdk/
│   └── rivet-python/
├── queries/
├── dev/
└── .github/workflows/
```

- `crates/rivet-core`: pure analysis library
- `crates/rivet-cli`: command line interface
- `crates/rivet-mcp`: MCP integration surface
- `crates/rivet-lsp`: LSP integration surface
- `crates/rivet-python`: Python bindings scaffold
- `crates/rivet-node`: Node bindings scaffold
- `crates/rivet-plugin-sdk`: plugin authoring scaffold
- `[workspace.dependencies]`: central place for shared dependency versions
- `[workspace.lints.clippy]`: workspace-wide Clippy policy and AI guardrails

## Quality Guardrails

- `cargo fmt --all --check` for formatting
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --all-features`
- GitHub CodeQL analysis for Rust projects via the `rust` workflow configuration
- Clippy cognitive complexity threshold capped at `10`

## Development

```bash
make setup      # Fetch workspace dependencies
make lint       # Run Trunk checks and workspace clippy
make format     # Run rustfmt and Trunk formatters
make test       # Run workspace tests
make build      # Build release artifacts for every member
make codeql     # Run local CodeQL analysis
```

## Current Status

The repository is being transformed from the workspace template into the Rivet
workspace described in [docs/core/system_design.md](./docs/core/system_design.md).
