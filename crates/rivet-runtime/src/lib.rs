use std::{
    collections::{HashMap, HashSet},
    env,
    fmt::Write as _,
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use anyhow::{Context, Result};
use globset::{Glob, GlobSet, GlobSetBuilder};
use ignore::WalkBuilder;
use rivet_core::{
    Analyzer, AnalyzerConfig, FileAnalysis, FileInput, Language, LanguageRegistry, LanguageSummary,
    ProjectAnalysis, ProjectSummary, RivetError, analysis_fingerprint,
};
use serde::{Deserialize, Serialize};

pub use rivet_core::{LanguageDescriptor, LanguageSource, LanguageSupportLevel};

#[derive(Debug, Clone, Default)]
pub struct AnalyzerBuildContext {
    pub config_dir: Option<PathBuf>,
    pub project_root: Option<PathBuf>,
    pub cache: RuntimeCacheConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum CacheMode {
    ReadOnly,
    #[default]
    ReadWrite,
}

impl CacheMode {
    const fn allows_writes(self) -> bool {
        matches!(self, Self::ReadWrite)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeCacheConfig {
    pub enabled: bool,
    pub dir: PathBuf,
    pub mode: CacheMode,
}

impl Default for RuntimeCacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            dir: PathBuf::from(".rivet/cache"),
            mode: CacheMode::ReadWrite,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CacheStats {
    pub hits: u32,
    pub misses: u32,
    pub writes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LanguageResolution {
    Full {
        language: Language,
        descriptor: LanguageDescriptor,
    },
    ParseOnly(LanguageDescriptor),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkippedFile {
    pub path: PathBuf,
    pub language_id: String,
    pub reason: String,
    pub support_level: Option<LanguageSupportLevel>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectedFiles {
    pub analyzable: Vec<FileInput>,
    pub skipped: Vec<SkippedFile>,
}

pub fn build_analyzer(config: AnalyzerConfig, context: &AnalyzerBuildContext) -> Result<Analyzer> {
    let plugin_config = config.plugins.clone();
    let mut analyzer = Analyzer::new(config)?;

    if !plugin_config.enabled {
        return Ok(analyzer);
    }

    for plugin_path in discover_plugin_paths(&plugin_config, context)? {
        let wasm_bytes = match fs::read(&plugin_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                eprintln!(
                    "rivet: failed to read plugin {}: {error}",
                    plugin_path.display()
                );
                continue;
            }
        };

        if let Err(error) = analyzer.register_plugin(&wasm_bytes) {
            eprintln!(
                "rivet: failed to register plugin {}: {error}",
                plugin_path.display()
            );
        }
    }

    Ok(analyzer)
}

pub fn analyze_files_with_cache(
    analyzer: &Analyzer,
    config: &AnalyzerConfig,
    context: &AnalyzerBuildContext,
    files: &[FileInput],
) -> Result<ProjectAnalysis> {
    analyze_files_with_cache_stats(analyzer, config, context, files)
        .map(|(analysis, _stats)| analysis)
}

pub fn analyze_files_with_cache_stats(
    analyzer: &Analyzer,
    config: &AnalyzerConfig,
    context: &AnalyzerBuildContext,
    files: &[FileInput],
) -> Result<(ProjectAnalysis, CacheStats)> {
    let mut analyses = Vec::with_capacity(files.len());
    let mut stats = CacheStats::default();

    for file in files {
        analyses.push(analyze_input_with_cache(
            analyzer, config, context, file, &mut stats,
        )?);
    }

    let threshold_violations = analyses
        .iter()
        .flat_map(|analysis| analyzer.check_file_thresholds(analysis))
        .collect::<Vec<_>>();

    Ok((
        ProjectAnalysis {
            summary: build_summary(&analyses),
            files: analyses,
            threshold_violations,
        },
        stats,
    ))
}

pub fn analyze_source_with_cache(
    analyzer: &Analyzer,
    config: &AnalyzerConfig,
    context: &AnalyzerBuildContext,
    source: &[u8],
    language: Language,
    file_path: Option<&Path>,
) -> Result<FileAnalysis> {
    analyze_source_with_cache_stats(analyzer, config, context, source, language, file_path)
        .map(|(analysis, _stats)| analysis)
}

pub fn analyze_source_with_cache_stats(
    analyzer: &Analyzer,
    config: &AnalyzerConfig,
    context: &AnalyzerBuildContext,
    source: &[u8],
    language: Language,
    file_path: Option<&Path>,
) -> Result<(FileAnalysis, CacheStats)> {
    let input = FileInput {
        file_path: file_path.map(Path::to_path_buf),
        language,
        source: source.to_vec(),
    };
    let mut stats = CacheStats::default();
    analyze_input_with_cache(analyzer, config, context, &input, &mut stats)
        .map(|analysis| (analysis, stats))
}

fn analyze_input_with_cache(
    analyzer: &Analyzer,
    config: &AnalyzerConfig,
    context: &AnalyzerBuildContext,
    input: &FileInput,
    stats: &mut CacheStats,
) -> Result<FileAnalysis> {
    let Some(file_path) = input.file_path.as_deref() else {
        return analyzer
            .analyze_source(&input.source, input.language, input.file_path.as_deref())
            .map_err(Into::into);
    };

    let Some(cache_dir) = cache_dir(context) else {
        return analyzer
            .analyze_source(&input.source, input.language, Some(file_path))
            .map_err(Into::into);
    };

    let cache_key = cache_key(config, input, file_path)?;
    let cache_path = cache_file_path(&cache_dir, &cache_key);

    if context.cache.enabled
        && cache_path.exists()
        && let Ok(analysis) = read_cached_analysis(&cache_path)
    {
        stats.hits = stats.hits.saturating_add(1);
        return Ok(analysis);
    }

    stats.misses = stats.misses.saturating_add(1);
    let analysis = analyzer
        .analyze_source(&input.source, input.language, Some(file_path))
        .map_err(anyhow::Error::from)?;

    if context.cache.enabled && context.cache.mode.allows_writes() {
        write_cached_analysis(&cache_path, &analysis)?;
        stats.writes = stats.writes.saturating_add(1);
    }

    Ok(analysis)
}

#[must_use]
pub fn available_languages() -> Vec<LanguageDescriptor> {
    runtime_available_languages().clone()
}

#[must_use]
pub fn supported_languages() -> Vec<Language> {
    runtime_supported_languages().clone()
}

#[must_use]
pub fn format_languages_text(languages: &[LanguageDescriptor]) -> String {
    let mut rendered = String::new();

    for language in languages {
        let extensions = if language.extensions.is_empty() {
            String::from("-")
        } else {
            language.extensions.join(", ")
        };

        let _ = writeln!(
            rendered,
            "{id:<16} {display:<20} {support:<11} {source:<14} {extensions}",
            id = language.id,
            display = language.display_name,
            support = language.support_level.as_str(),
            source = language.source.as_str(),
        );
    }

    rendered
}

pub fn classify_path(
    path: &Path,
    language_override: Option<&str>,
) -> std::result::Result<LanguageResolution, RivetError> {
    if let Some(language) = language_override {
        return resolve_language(language);
    }

    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .ok_or_else(|| RivetError::UnsupportedLanguage(path.display().to_string()))?;

    classify_language_slug(extension)
        .ok_or_else(|| RivetError::UnsupportedLanguage(extension.to_string()))
}

pub fn collect_files(
    path: &Path,
    language_override: Option<&str>,
    glob: Option<&str>,
) -> Result<CollectedFiles> {
    if path.is_file() {
        return collect_file(path, language_override);
    }

    let matcher = compile_glob_matcher(glob)?;
    let mut analyzable = Vec::new();
    let mut skipped = Vec::new();

    for entry in WalkBuilder::new(path).standard_filters(true).build() {
        let entry = entry?;
        if !entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            continue;
        }

        let candidate = entry.into_path();
        if !matches_glob(path, &candidate, matcher.as_ref()) {
            continue;
        }

        match classify_path(&candidate, language_override) {
            Ok(LanguageResolution::Full { language, .. }) => {
                analyzable.push(build_file_input(&candidate, language)?);
            }
            Ok(LanguageResolution::ParseOnly(descriptor)) => {
                skipped.push(SkippedFile {
                    path: candidate,
                    language_id: descriptor.id,
                    reason: "recognized but parse-only".to_string(),
                    support_level: Some(LanguageSupportLevel::ParseOnly),
                });
            }
            Err(RivetError::UnsupportedLanguage(language_id)) => {
                skipped.push(SkippedFile {
                    path: candidate,
                    language_id,
                    reason: "unsupported language".to_string(),
                    support_level: None,
                });
            }
            Err(error) => return Err(error.into()),
        }
    }

    Ok(CollectedFiles {
        analyzable,
        skipped,
    })
}

pub fn collect_file(path: &Path, language_override: Option<&str>) -> Result<CollectedFiles> {
    match classify_path(path, language_override) {
        Ok(LanguageResolution::Full { language, .. }) => Ok(CollectedFiles {
            analyzable: vec![build_file_input(path, language)?],
            skipped: Vec::new(),
        }),
        Ok(LanguageResolution::ParseOnly(descriptor)) => Ok(CollectedFiles {
            analyzable: Vec::new(),
            skipped: vec![SkippedFile {
                path: path.to_path_buf(),
                language_id: descriptor.id,
                reason: "recognized but parse-only".to_string(),
                support_level: Some(LanguageSupportLevel::ParseOnly),
            }],
        }),
        Err(RivetError::UnsupportedLanguage(language_id)) => Ok(CollectedFiles {
            analyzable: Vec::new(),
            skipped: vec![SkippedFile {
                path: path.to_path_buf(),
                language_id,
                reason: "unsupported language".to_string(),
                support_level: None,
            }],
        }),
        Err(error) => Err(error.into()),
    }
}

pub fn resolve_language(value: &str) -> std::result::Result<LanguageResolution, RivetError> {
    let slug = value.to_ascii_lowercase();
    if let Ok(language) = slug.parse::<Language>() {
        return Ok(LanguageResolution::Full {
            language,
            descriptor: full_language_descriptor(language),
        });
    }

    if let Some(descriptor) = parse_only_language_by_slug(&slug) {
        return Ok(LanguageResolution::ParseOnly(descriptor));
    }

    Err(RivetError::UnsupportedLanguage(value.to_string()))
}

fn classify_language_slug(slug: &str) -> Option<LanguageResolution> {
    let slug = slug.to_ascii_lowercase();
    if let Ok(language) = slug.parse::<Language>() {
        return Some(LanguageResolution::Full {
            language,
            descriptor: full_language_descriptor(language),
        });
    }

    parse_only_language_by_extension(&slug).map(LanguageResolution::ParseOnly)
}

fn build_file_input(path: &Path, language: Language) -> Result<FileInput> {
    let source = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    Ok(FileInput {
        file_path: Some(path.to_path_buf()),
        language,
        source,
    })
}

fn build_summary(files: &[FileAnalysis]) -> ProjectSummary {
    let mut languages = HashMap::new();
    let total_files = usize_to_u32(files.len());
    let total_functions = files
        .iter()
        .map(|file| usize_to_u32(file.functions.len()))
        .sum();
    let total_nloc = files.iter().map(|file| file.file_metrics.nloc).sum();
    let avg_cyclomatic = average(files.iter().flat_map(|file| {
        file.functions
            .iter()
            .map(|function| f64::from(function.cyclomatic_complexity))
    }));
    let avg_cognitive = average(files.iter().flat_map(|file| {
        file.functions
            .iter()
            .map(|function| f64::from(function.cognitive_complexity))
    }));
    let avg_maintainability_index = average(
        files
            .iter()
            .map(|file| file.file_metrics.maintainability_index),
    );

    for file in files {
        let entry = languages
            .entry(file.language)
            .or_insert_with(LanguageSummary::default);
        entry.files += 1;
        entry.functions += usize_to_u32(file.functions.len());
        entry.nloc += file.file_metrics.nloc;
    }

    ProjectSummary {
        total_files,
        total_functions,
        total_nloc,
        avg_cyclomatic,
        avg_cognitive,
        avg_maintainability_index,
        languages,
    }
}

fn average(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, count) = values.fold((0.0, 0_u32), |(sum, count), value| {
        (sum + value, count.saturating_add(1))
    });
    if count == 0 {
        0.0
    } else {
        sum / f64::from(count)
    }
}

fn usize_to_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn cache_dir(context: &AnalyzerBuildContext) -> Option<PathBuf> {
    if !context.cache.enabled {
        return None;
    }

    Some(if context.cache.dir.is_absolute() {
        context.cache.dir.clone()
    } else {
        context
            .project_root
            .as_deref()
            .unwrap_or_else(|| Path::new("."))
            .join(&context.cache.dir)
    })
}

fn cache_key(config: &AnalyzerConfig, input: &FileInput, file_path: &Path) -> Result<String> {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    analysis_fingerprint().hash(&mut hasher);
    file_path.to_string_lossy().hash(&mut hasher);
    input.language.as_str().hash(&mut hasher);
    input.source.hash(&mut hasher);
    serde_json::to_vec(config)
        .context("failed to serialize analyzer config for cache key")?
        .hash(&mut hasher);
    Ok(format!("{:016x}", hasher.finish()))
}

fn cache_file_path(cache_dir: &Path, cache_key: &str) -> PathBuf {
    cache_dir.join(format!("{cache_key}.json"))
}

fn read_cached_analysis(cache_path: &Path) -> Result<FileAnalysis> {
    let bytes = fs::read(cache_path)
        .with_context(|| format!("failed to read cache entry {}", cache_path.display()))?;
    serde_json::from_slice(&bytes)
        .with_context(|| format!("failed to deserialize cache entry {}", cache_path.display()))
}

fn write_cached_analysis(cache_path: &Path, analysis: &FileAnalysis) -> Result<()> {
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create cache directory {}", parent.display()))?;
    }

    let bytes = serde_json::to_vec(analysis)
        .with_context(|| format!("failed to serialize cache entry {}", cache_path.display()))?;
    fs::write(cache_path, bytes)
        .with_context(|| format!("failed to write cache entry {}", cache_path.display()))
}

fn compile_glob_matcher(glob: Option<&str>) -> Result<Option<GlobSet>> {
    let Some(glob) = glob else {
        return Ok(None);
    };

    let mut builder = GlobSetBuilder::new();
    let compiled = Glob::new(glob).with_context(|| format!("invalid glob `{glob}`"))?;
    builder.add(compiled);
    builder
        .build()
        .map(Some)
        .context("failed to compile glob matcher")
}

fn matches_glob(root: &Path, candidate: &Path, matcher: Option<&GlobSet>) -> bool {
    let Some(matcher) = matcher else {
        return true;
    };

    candidate
        .strip_prefix(root)
        .ok()
        .is_some_and(|relative| matcher.is_match(relative))
        || matcher.is_match(candidate)
}

fn full_language_descriptor(language: Language) -> LanguageDescriptor {
    runtime_available_languages()
        .iter()
        .find(|descriptor| {
            descriptor.support_level == LanguageSupportLevel::Full
                && descriptor.id == language.as_str()
        })
        .cloned()
        .expect("built-in language should always have a descriptor")
}

fn parse_only_language_by_slug(slug: &str) -> Option<LanguageDescriptor> {
    runtime_available_languages().iter().find_map(|descriptor| {
        (descriptor.support_level == LanguageSupportLevel::ParseOnly
            && (descriptor.id == slug
                || descriptor
                    .extensions
                    .iter()
                    .any(|extension| extension == slug)))
        .then(|| descriptor.clone())
    })
}

fn parse_only_language_by_extension(extension: &str) -> Option<LanguageDescriptor> {
    runtime_available_languages().iter().find_map(|descriptor| {
        (descriptor.support_level == LanguageSupportLevel::ParseOnly
            && descriptor.extensions.iter().any(|known| known == extension))
        .then(|| descriptor.clone())
    })
}

fn runtime_available_languages() -> &'static Vec<LanguageDescriptor> {
    static AVAILABLE: OnceLock<Vec<LanguageDescriptor>> = OnceLock::new();
    AVAILABLE.get_or_init(|| {
        LanguageRegistry::new()
            .expect("language registry should build")
            .available_languages()
    })
}

fn runtime_supported_languages() -> &'static Vec<Language> {
    static SUPPORTED: OnceLock<Vec<Language>> = OnceLock::new();
    SUPPORTED.get_or_init(|| {
        LanguageRegistry::new()
            .expect("language registry should build")
            .supported_languages()
    })
}

fn discover_plugin_paths(
    plugin_config: &rivet_core::PluginConfig,
    context: &AnalyzerBuildContext,
) -> Result<Vec<PathBuf>> {
    let mut discovered = Vec::new();
    let mut seen = HashSet::new();

    for entry in plugin_config.entries.iter().filter(|entry| entry.enabled) {
        push_plugin_candidate(
            resolve_path(&entry.path, context.config_dir.as_deref()),
            &mut discovered,
            &mut seen,
        )?;
    }

    for discovery_path in &plugin_config.discovery_paths {
        push_plugin_candidate(
            resolve_path(discovery_path, context.config_dir.as_deref()),
            &mut discovered,
            &mut seen,
        )?;
    }

    if let Some(project_root) = context.project_root.as_deref() {
        push_plugin_candidate(
            project_root.join(".rivet/plugins"),
            &mut discovered,
            &mut seen,
        )?;
    }

    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        push_plugin_candidate(home.join(".rivet/plugins"), &mut discovered, &mut seen)?;
    }

    Ok(discovered)
}

fn push_plugin_candidate(
    path: PathBuf,
    discovered: &mut Vec<PathBuf>,
    seen: &mut HashSet<PathBuf>,
) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_file() {
        if is_wasm_file(&path) {
            insert_plugin_path(path, discovered, seen);
        }
        return Ok(());
    }

    let mut entries = fs::read_dir(&path)
        .with_context(|| format!("failed to read plugin directory {}", path.display()))?
        .map(|entry| entry.map(|value| value.path()))
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to read plugin directory {}", path.display()))?;
    entries.sort();

    for entry in entries {
        if entry.is_file() && is_wasm_file(&entry) {
            insert_plugin_path(entry, discovered, seen);
        }
    }

    Ok(())
}

fn insert_plugin_path(path: PathBuf, discovered: &mut Vec<PathBuf>, seen: &mut HashSet<PathBuf>) {
    let dedupe_key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
    if seen.insert(dedupe_key) {
        discovered.push(path);
    }
}

fn resolve_path(path: &Path, base_dir: Option<&Path>) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir.map_or_else(|| path.to_path_buf(), |base| base.join(path))
    }
}

fn is_wasm_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("wasm"))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;
    use rivet_core::Thresholds;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rivet-{name}-{suffix}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn available_languages_includes_parse_only_entries() {
        let available = available_languages();
        let supported = supported_languages();

        assert!(available.len() > supported.len());
        assert!(available.iter().any(|language| language.id == "rust"));
        assert!(available.iter().any(|language| language.id == "swift"));
        assert!(
            available
                .iter()
                .any(|language| language.support_level == LanguageSupportLevel::ParseOnly)
        );
    }

    #[test]
    fn collect_files_reports_parse_only_files_as_skipped() {
        let root = unique_temp_dir("collect-parse-only");
        let path = root.join("sample.swift");
        fs::write(&path, "func sample(value: Int) -> Int { value }").expect("write source");

        let collected = collect_files(&path, None, None).expect("collect");
        assert!(collected.analyzable.is_empty());
        assert_eq!(collected.skipped.len(), 1);
        assert_eq!(collected.skipped[0].language_id, "swift");
        assert_eq!(
            collected.skipped[0].support_level,
            Some(LanguageSupportLevel::ParseOnly)
        );

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn collect_files_accepts_full_language_files() {
        let root = unique_temp_dir("collect-full");
        let path = root.join("sample.rs");
        fs::write(&path, "fn sample() {}").expect("write source");

        let collected = collect_files(&path, None, None).expect("collect");
        assert_eq!(collected.analyzable.len(), 1);
        assert!(collected.skipped.is_empty());
        assert_eq!(collected.analyzable[0].language, Language::Rust);

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn format_languages_text_mentions_support_levels() {
        let rendered = format_languages_text(&available_languages());
        assert!(rendered.contains("rust"));
        assert!(rendered.contains("full"));
        assert!(rendered.contains("parse_only"));
    }

    #[test]
    fn analyze_source_with_cache_reports_hit_and_write_stats() {
        let root = unique_temp_dir("cache-hit");
        let source_path = root.join("sample.rs");
        fs::write(&source_path, "fn sample(value: i32) -> i32 { value + 1 }")
            .expect("write source");

        let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let context = AnalyzerBuildContext {
            project_root: Some(root.clone()),
            cache: RuntimeCacheConfig {
                dir: PathBuf::from(".rivet/cache"),
                ..RuntimeCacheConfig::default()
            },
            ..AnalyzerBuildContext::default()
        };

        let (_, cold_stats) = analyze_source_with_cache_stats(
            &analyzer,
            &AnalyzerConfig::default(),
            &context,
            b"fn sample(value: i32) -> i32 { value + 1 }",
            Language::Rust,
            Some(&source_path),
        )
        .expect("cold analysis");
        assert_eq!(cold_stats.hits, 0);
        assert_eq!(cold_stats.misses, 1);
        assert_eq!(cold_stats.writes, 1);

        let (_, warm_stats) = analyze_source_with_cache_stats(
            &analyzer,
            &AnalyzerConfig::default(),
            &context,
            b"fn sample(value: i32) -> i32 { value + 1 }",
            Language::Rust,
            Some(&source_path),
        )
        .expect("warm analysis");
        assert_eq!(warm_stats.hits, 1);
        assert_eq!(warm_stats.misses, 0);
        assert_eq!(warm_stats.writes, 0);

        fs::remove_dir_all(&root).expect("cleanup");
    }

    #[test]
    fn cache_key_changes_when_analyzer_config_changes() {
        let root = unique_temp_dir("cache-config");
        let source_path = root.join("sample.rs");
        fs::write(&source_path, "fn sample() { println!(\"hi\"); }").expect("write source");

        let analyzer = Analyzer::new(AnalyzerConfig::default()).expect("analyzer");
        let context = AnalyzerBuildContext {
            project_root: Some(root.clone()),
            cache: RuntimeCacheConfig {
                dir: PathBuf::from(".rivet/cache"),
                ..RuntimeCacheConfig::default()
            },
            ..AnalyzerBuildContext::default()
        };

        let base_config = AnalyzerConfig::default();
        let (_, first_stats) = analyze_source_with_cache_stats(
            &analyzer,
            &base_config,
            &context,
            b"fn sample() { println!(\"hi\"); }",
            Language::Rust,
            Some(&source_path),
        )
        .expect("first analysis");
        assert_eq!(first_stats.writes, 1);

        let mut changed_config = AnalyzerConfig::default();
        changed_config.thresholds = Thresholds {
            max_cyclomatic_complexity: Some(1),
            ..Thresholds::default()
        };
        let (_, changed_stats) = analyze_source_with_cache_stats(
            &analyzer,
            &changed_config,
            &context,
            b"fn sample() { println!(\"hi\"); }",
            Language::Rust,
            Some(&source_path),
        )
        .expect("changed analysis");
        assert_eq!(changed_stats.hits, 0);
        assert_eq!(changed_stats.misses, 1);
        assert_eq!(changed_stats.writes, 1);

        fs::remove_dir_all(&root).expect("cleanup");
    }
}
