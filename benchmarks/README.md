# Benchmarks

This directory collects benchmark entrypoints and operator notes for Rivet performance work.

Current benchmark entrypoints:

```bash
cargo bench -p rivet-runtime --bench cache_roundtrip
cargo bench -p rivet-lsp --bench cached_reads
```

For the performance targets, cache notes, and the current plugin release gate policy, see [docs/core/performance.md](/Users/yu/local/src/github/rivet/docs/core/performance.md).
