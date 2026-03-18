# Performance

Rivet's performance work is centered on fast cached reads in the editor and predictable per-file analysis for larger repositories.

## Targets

These are the targets called out in `system_design.md` and used as the baseline for local benchmarking:

- Single file analysis, 1K LOC: under 10ms
- Single file analysis, 10K LOC: under 100ms
- Cold start CLI: under 200ms
- MCP startup: under 100ms
- LSP startup: under 150ms
- LSP diagnostics on save: under 200ms
- LSP diagnostics on change: under 500ms
- LSP code lens response: under 100ms
- LSP hover response: under 50ms

## Local Benchmark Commands

Run the runtime benchmark suite:

```bash
cargo bench -p rivet-runtime --bench cache_roundtrip
```

Run the LSP benchmark suite:

```bash
cargo bench -p rivet-lsp --bench cached_reads
```

Compile the benchmark without running it:

```bash
cargo bench -p rivet-lsp --bench cached_reads --no-run
```

Run the LSP crate tests that cover cached document reads and revision guards:

```bash
cargo test -p rivet-lsp
```

## Notes

- The benchmark scaffold currently measures in-memory cached document reads and revision-guarded updates.
- The runtime benchmark covers single-file direct analysis plus cold and warm directory scans with the persistent cache path.
- The benchmark workflow is defined in `.github/workflows/performance.yml` and runs on `workflow_dispatch` plus a weekly schedule; PRs use a compile-only smoke job from `.github/workflows/test.yml`.
- Persistent cache integration is live in the runtime and is exercised by the warm directory benchmark path.
- The plugin runtime remains release-gated while upstream Extism/Wasmtime advisories are unresolved; performance work does not change that policy.
