# 3. WASM-based Plugin System via Extism

Date: 2026-03-17

## Status

Accepted

## Context

Rivet needs a way to allow users and the community to extend the tool with custom complexity metrics. Hard-coding every possible metric into the core library is unsustainable and limits the tool's flexibility.

Requirements for the plugin system:

1.  **Safety**: Plugins should not have arbitrary access to the host's filesystem, network, or environment.
2.  **Language Agnosticism**: Authors should be able to write plugins in their language of choice (Rust, Go, Zig, C, etc.).
3.  **Performance**: The overhead of calling a plugin should be low enough for large-scale analysis.
4.  **Stability**: The host should be able to limit a plugin's memory usage and execution time.

## Decision

We will implement a **WASM-based plugin system using Extism**.

Extism provides a high-level abstraction over Wasmtime, making it easy to embed a WASM runtime and define a clear interface between the host and plugins.

Key implementation details:

- **Sandboxing**: Plugins run in a restricted WASM environment with no ambient authority.
- **Interface**: Communication will happen via JSON-serialized data passed through linear memory.
- **SDK**: We will provide a `rivet-plugin-sdk` to simplify plugin development for common languages.
- **Resource Limits**: The `PluginHost` in `rivet-core` will enforce configurable memory (default 16MB) and CPU (default 5s) limits per plugin call.

## Consequences

### Positive

- **Security**: Near-perfect isolation between the host and untrusted plugin code.
- **Portability**: Plugins compiled to `.wasm` work across any OS/architecture supported by Rivet.
- **Ease of Use**: Extism handles the complex details of WASI and memory management.
- **Language Diversity**: Leverages the growing ecosystem of languages targeting WASM.

### Negative / Risks

- **Serialization Overhead**: Passing large ASTs or source fragments via JSON introduces overhead.
- **WASM Ecosystem**: Some languages still have immature WASM support or large runtimes (e.g., Python, Ruby).
- **Debugging**: Debugging WASM plugins can be more difficult than native code.
