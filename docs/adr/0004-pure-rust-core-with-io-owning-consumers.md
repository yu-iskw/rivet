# 4. Pure-Rust Core with IO-owning Consumers

Date: 2026-03-17

## Status

Accepted

## Context

Rivet is designed to be embedded in multiple environments:

1.  **CLI**: A standalone binary for manual or CI/CD use.
2.  **MCP Server**: A tool for AI agents (Claude, Cursor).
3.  **LSP Server**: A real-time engine for IDEs.
4.  **Language Bindings**: Native modules for Python and Node.js.

If the core analysis logic is coupled to the filesystem, networking, or a specific asynchronous runtime (like Tokio), it becomes difficult to maintain consistency and portability across these consumers. For example, the LSP server needs async/await and in-memory state, while the Python bindings require synchronous FFI calls.

## Decision

We will adopt a **"Functional Core, Imperative Shell"** architecture.

- **`rivet-core` (Functional Core)**: This crate will be a pure Rust library. It will have no knowledge of the filesystem, no network dependencies, and no asynchronous runtime. Its primary entry points will take source code as byte slices (`&[u8]`) and return structured results as plain Rust types.
- **Consumers (Imperative Shell)**: Each consumer crate (`rivet-cli`, `rivet-mcp`, `rivet-lsp`, `rivet-python`, `rivet-node`) will own its IO. They will be responsible for:
  - Walking the filesystem (using `ignore`).
  - Reading files into memory.
  - Managing concurrency (using `rayon` or `tokio`).
  - Handling transport protocols (stdio, JSON-RPC, FFI).
  - Formatting output (SARIF, JSON, CSV).

## Consequences

### Positive

- **Testability**: `rivet-core` becomes trivially testable with standard Rust unit tests — no mocks or complex IO setup required.
- **Portability**: The core can be compiled to WASM for browser use or embedded in any environment without pulling in heavy IO dependencies.
- **Consistency**: A bug in the analysis logic is fixed in one place and benefits all interfaces simultaneously.
- **Performance**: The core can focus on raw CPU-bound analysis without being blocked by IO latency.

### Negative / Risks

- **Boilerplate**: Each consumer must implement its own logic for reading files and handling parallelism, though this can be mitigated by shared utility crates if needed.
- **Memory Usage**: Since the core takes byte slices, consumers must ensure they don't load too many large files into memory at once during parallel analysis.
