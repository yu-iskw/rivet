use std::{
    env, fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use rivet_core::{Analyzer, AnalyzerConfig, Language};
use rivet_runtime::{
    AnalyzerBuildContext, RuntimeCacheConfig, analyze_files_with_cache, collect_files,
};

fn unique_temp_dir(name: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let path = env::temp_dir().join(format!("rivet-bench-{name}-{suffix}"));
    fs::create_dir_all(&path).expect("temp dir");
    path
}

fn small_rust_source() -> &'static str {
    "fn classify(value: i32) -> i32 {\n    if value > 10 {\n        value + 1\n    } else {\n        value - 1\n    }\n}\n"
}

fn large_rust_source() -> String {
    let mut source = String::new();
    for index in 0..800 {
        source.push_str(&format!(
            "fn generated_{index}(value: i32) -> i32 {{\n    if value % 2 == 0 {{ value + {index} }} else {{ value - {index} }}\n}}\n\n"
        ));
    }
    source
}

fn cache_context(root: &PathBuf) -> AnalyzerBuildContext {
    AnalyzerBuildContext {
        project_root: Some(root.clone()),
        cache: RuntimeCacheConfig {
            dir: PathBuf::from(".rivet/cache"),
            ..RuntimeCacheConfig::default()
        },
        ..AnalyzerBuildContext::default()
    }
}

fn bench_runtime_paths(c: &mut Criterion) {
    let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
    let config = AnalyzerConfig::default();
    let large_source = large_rust_source();

    c.bench_function("single_file_small_direct", |b| {
        b.iter(|| {
            black_box(
                analyzer
                    .analyze_source(small_rust_source().as_bytes(), Language::Rust, None)
                    .expect("small analysis"),
            );
        });
    });

    c.bench_function("single_file_large_direct", |b| {
        b.iter(|| {
            black_box(
                analyzer
                    .analyze_source(large_source.as_bytes(), Language::Rust, None)
                    .expect("large analysis"),
            );
        });
    });

    let cold_root = unique_temp_dir("cold-directory");
    for index in 0..12 {
        let path = cold_root.join(format!("file_{index}.rs"));
        fs::write(&path, small_rust_source()).expect("write cold fixture");
    }
    let cold_inputs = collect_files(&cold_root, None, None)
        .expect("collect cold fixtures")
        .analyzable;

    c.bench_function("directory_cold_cache", |b| {
        b.iter(|| {
            let run_root = unique_temp_dir("cold-run");
            let context = cache_context(&run_root);
            black_box(
                analyze_files_with_cache(&analyzer, &config, &context, &cold_inputs)
                    .expect("cold project analysis"),
            );
        });
    });

    let warm_root = unique_temp_dir("warm-directory");
    for index in 0..12 {
        let path = warm_root.join(format!("file_{index}.rs"));
        fs::write(&path, small_rust_source()).expect("write warm fixture");
    }
    let warm_context = cache_context(&warm_root);
    let warm_inputs = collect_files(&warm_root, None, None)
        .expect("collect warm fixtures")
        .analyzable;
    analyze_files_with_cache(&analyzer, &config, &warm_context, &warm_inputs)
        .expect("prime warm cache");

    c.bench_function("directory_warm_cache", |b| {
        b.iter(|| {
            black_box(
                analyze_files_with_cache(&analyzer, &config, &warm_context, &warm_inputs)
                    .expect("warm project analysis"),
            );
        });
    });

    let _ = fs::remove_dir_all(cold_root);
}

criterion_group!(cache_roundtrip, bench_runtime_paths);
criterion_main!(cache_roundtrip);
