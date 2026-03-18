use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rivet_core::{Analyzer, AnalyzerConfig, Language};
use rivet_lsp::state::DocumentState;

fn sample_analysis() -> rivet_core::FileAnalysis {
    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer should build");
    analyzer
        .analyze_source(
            b"fn bench_example(a: i32) { if a > 0 { println!(\"{a}\"); } }\n",
            Language::Rust,
            None,
        )
        .expect("fixture should analyze")
}

fn bench_cached_document_reads(c: &mut Criterion) {
    let state = DocumentState::new();
    let analysis = sample_analysis();
    let uri = "file:///tmp/rivet-lsp-bench.rs";
    state.open(
        uri,
        "fn bench_example() {}\n".to_string(),
        1,
        Language::Rust,
    );
    let _ = state.set_analysis_if_revision(uri, 1, Some(analysis));

    c.bench_function("document_state_cached_read", |b| {
        b.iter(|| {
            black_box(state.get(uri));
        });
    });

    c.bench_function("document_state_revision_guard", |b| {
        b.iter(|| {
            black_box(state.set_analysis_if_revision(uri, 1, None));
        });
    });
}

criterion_group!(cached_reads, bench_cached_document_reads);
criterion_main!(cached_reads);
