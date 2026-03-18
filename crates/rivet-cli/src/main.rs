#![allow(clippy::missing_const_for_fn)]

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use rivet_core::{
    Analyzer, AnalyzerConfig, PathThresholdOverride, PluginConfig, PluginEntryConfig,
    ProjectAnalysis, Thresholds,
    output::{to_csv, to_json, to_sarif, to_text},
};
use rivet_runtime::{
    AnalyzerBuildContext, CacheMode, RuntimeCacheConfig, analyze_files_with_cache,
    available_languages, build_analyzer, collect_files, format_languages_text,
};

#[derive(Debug, Parser)]
#[command(author, version, about = "AI-agent-native code complexity analyzer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[arg(short, long)]
    language: Option<String>,
    #[arg(short, long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
    #[arg(short, long)]
    output: Option<PathBuf>,
    #[arg(short, long)]
    config: Option<PathBuf>,
}

#[derive(Debug, Clone, Subcommand)]
enum Command {
    Analyze {
        #[arg(default_value = ".")]
        path: PathBuf,
    },
    Check {
        #[arg(default_value = ".")]
        path: PathBuf,
        #[arg(short = 'C', long)]
        max_cc: Option<u32>,
        #[arg(long)]
        max_cognitive: Option<u32>,
        #[arg(short = 'a', long)]
        max_params: Option<u32>,
        #[arg(long)]
        max_length: Option<u32>,
        #[arg(long)]
        max_nesting: Option<u32>,
        #[arg(long)]
        warning_only: bool,
    },
    Languages,
    Metrics,
    Serve,
    Lsp {
        #[arg(long)]
        analyze_on_change: Option<bool>,
        #[arg(long)]
        debounce_ms: Option<u64>,
        #[arg(long)]
        enable_code_lenses: Option<bool>,
        #[arg(long)]
        enable_hover: Option<bool>,
        #[arg(long, value_enum)]
        diagnostic_severity: Option<DiagnosticSeverityArg>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
    Csv,
    Sarif,
}

#[derive(Debug, Clone, Copy, ValueEnum, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum DiagnosticSeverityArg {
    Warning,
    Information,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct ConfigFile {
    thresholds: Option<ThresholdsConfigFile>,
    plugins: Option<PluginsConfigFile>,
    cache: Option<CacheConfigFile>,
    lsp: Option<LspConfigFile>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct ThresholdsConfigFile {
    #[serde(flatten)]
    thresholds: PartialThresholds,
    #[serde(default)]
    overrides: Vec<ThresholdOverrideFile>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct ThresholdOverrideFile {
    #[serde(default)]
    paths: Vec<String>,
    #[serde(flatten)]
    thresholds: PartialThresholds,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct LspConfigFile {
    analyze_on_change: Option<bool>,
    debounce_ms: Option<u64>,
    enable_code_lenses: Option<bool>,
    enable_hover: Option<bool>,
    diagnostic_severity: Option<DiagnosticSeverityArg>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PluginsConfigFile {
    enabled: Option<bool>,
    #[serde(default)]
    discovery_paths: Vec<PathBuf>,
    #[serde(default)]
    entries: Vec<PluginEntryFile>,
    max_memory_pages: Option<u32>,
    timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct CacheConfigFile {
    enabled: Option<bool>,
    dir: Option<PathBuf>,
    mode: Option<CacheMode>,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct PluginEntryFile {
    path: PathBuf,
    name: Option<String>,
    enabled: Option<bool>,
}

#[derive(Debug, Clone, Default, serde::Deserialize)]
struct PartialThresholds {
    max_cyclomatic_complexity: Option<u32>,
    max_cognitive_complexity: Option<u32>,
    max_function_length: Option<u32>,
    max_parameter_count: Option<u32>,
    max_nesting_depth: Option<u32>,
    min_maintainability_index: Option<f64>,
}

struct LoadedConfigFile {
    path: PathBuf,
    file: ConfigFile,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.clone().unwrap_or_else(|| Command::Analyze {
        path: PathBuf::from("."),
    });

    match command {
        Command::Analyze { path } => {
            let (config, build_context) = load_config(&cli, None)?;
            let analyzer = build_analyzer(config.clone(), &build_context)?;
            let analysis = analyze_path(
                &analyzer,
                &config,
                &build_context,
                &path,
                cli.language.as_deref(),
            )?;
            emit_output(&cli, &analysis)
        }
        Command::Check {
            path,
            max_cc,
            max_cognitive,
            max_params,
            max_length,
            max_nesting,
            warning_only,
        } => {
            let overrides = PartialThresholds {
                max_cyclomatic_complexity: max_cc,
                max_cognitive_complexity: max_cognitive,
                max_function_length: max_length,
                max_parameter_count: max_params,
                max_nesting_depth: max_nesting,
                ..PartialThresholds::default()
            };
            let (config, build_context) = load_config(&cli, Some(overrides))?;
            let analyzer = build_analyzer(config.clone(), &build_context)?;
            let analysis = analyze_path(
                &analyzer,
                &config,
                &build_context,
                &path,
                cli.language.as_deref(),
            )?;
            emit_output(&cli, &analysis)?;
            let result = analyzer.check_thresholds(&analysis);
            if !warning_only && !result.passed {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Languages => {
            print!("{}", render_languages(cli.format)?);
            Ok(())
        }
        Command::Metrics => {
            for metric in [
                "cyclomatic_complexity",
                "cognitive_complexity",
                "halstead",
                "lines_of_code",
                "parameter_count",
                "maintainability_index",
                "nesting_depth",
            ] {
                println!("{metric}");
            }
            Ok(())
        }
        Command::Serve => {
            let (config, build_context) = load_config(&cli, None)?;
            rivet_mcp::run_stdio_with_context(config, build_context)
        }
        Command::Lsp {
            analyze_on_change,
            debounce_ms,
            enable_code_lenses,
            enable_hover,
            diagnostic_severity,
        } => {
            let (config, build_context) = load_lsp_config(
                &cli,
                analyze_on_change,
                debounce_ms,
                enable_code_lenses,
                enable_hover,
                diagnostic_severity,
            )?;
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(rivet_lsp::run_stdio_with_context(config, build_context))
        }
    }
}

fn analyze_path(
    analyzer: &Analyzer,
    config: &AnalyzerConfig,
    build_context: &AnalyzerBuildContext,
    path: &Path,
    language_override: Option<&str>,
) -> Result<ProjectAnalysis> {
    let collected = collect_files(path, language_override, None)?;
    for skipped in &collected.skipped {
        eprintln!(
            "rivet: skipping {} ({})",
            skipped.path.display(),
            skipped.reason
        );
    }
    if collected.analyzable.is_empty() {
        if path.is_file() && !collected.skipped.is_empty() {
            let skipped = &collected.skipped[0];
            anyhow::bail!("{}: {}", skipped.language_id, skipped.reason);
        }
        anyhow::bail!("no supported source files found under {}", path.display());
    }
    analyze_files_with_cache(analyzer, config, build_context, &collected.analyzable)
}

fn emit_output(cli: &Cli, analysis: &ProjectAnalysis) -> Result<()> {
    let rendered = match cli.format {
        OutputFormat::Json => to_json(analysis)?,
        OutputFormat::Text => to_text(analysis),
        OutputFormat::Csv => to_csv(analysis),
        OutputFormat::Sarif => to_sarif(analysis)?,
    };

    if let Some(path) = &cli.output {
        fs::write(path, rendered).with_context(|| format!("failed to write {}", path.display()))?;
    } else {
        println!("{rendered}");
    }

    Ok(())
}

fn render_languages(format: OutputFormat) -> Result<String> {
    let languages = available_languages();
    match format {
        OutputFormat::Json => serde_json::to_string_pretty(&languages).map_err(Into::into),
        _ => Ok(format_languages_text(&languages)),
    }
}

fn load_config(
    cli: &Cli,
    threshold_overrides: Option<PartialThresholds>,
) -> Result<(AnalyzerConfig, AnalyzerBuildContext)> {
    let mut config = AnalyzerConfig::default();
    let loaded_config = read_config_file(cli.config.clone())?;
    let mut build_context = AnalyzerBuildContext::default();

    if let Some(file) = &loaded_config
        && let Some(thresholds) = &file.file.thresholds
    {
        thresholds.thresholds.apply_to(&mut config.thresholds);
        config.threshold_overrides = thresholds
            .overrides
            .iter()
            .cloned()
            .map(ThresholdOverrideFile::into_core_override)
            .collect();
    }
    if let Some(file) = &loaded_config
        && let Some(plugins) = &file.file.plugins
    {
        config.plugins = plugins.clone().into_core_plugin_config();
    }
    if let Some(file) = &loaded_config
        && let Some(cache) = &file.file.cache
    {
        build_context.cache = cache.clone().into_runtime_cache_config();
    }

    if let Ok(value) = env::var("RIVET_MAX_CC") {
        config.thresholds.max_cyclomatic_complexity = value.parse().ok();
    }
    if let Ok(value) = env::var("RIVET_MAX_COGNITIVE") {
        config.thresholds.max_cognitive_complexity = value.parse().ok();
    }

    if let Some(overrides) = threshold_overrides {
        overrides.apply_to(&mut config.thresholds);
    }

    let project_root = loaded_config
        .as_ref()
        .and_then(|file| file.path.parent().map(Path::to_path_buf))
        .unwrap_or(env::current_dir()?);

    build_context.config_dir = loaded_config
        .as_ref()
        .and_then(|file| file.path.parent().map(Path::to_path_buf));
    build_context.project_root = Some(project_root);

    Ok((config, build_context))
}

fn load_lsp_config(
    cli: &Cli,
    analyze_on_change: Option<bool>,
    debounce_ms: Option<u64>,
    enable_code_lenses: Option<bool>,
    enable_hover: Option<bool>,
    diagnostic_severity: Option<DiagnosticSeverityArg>,
) -> Result<(rivet_lsp::LspConfig, AnalyzerBuildContext)> {
    let config_file = read_config_file(cli.config.clone())?;
    let lsp_config = config_file.as_ref().and_then(|file| file.file.lsp.as_ref());
    let (analyzer_config, build_context) = load_config(cli, None)?;

    Ok((
        rivet_lsp::LspConfig {
            analyzer_config,
            analyze_on_change: analyze_on_change
                .or_else(|| lsp_config.and_then(|config| config.analyze_on_change))
                .unwrap_or(true),
            debounce_ms: debounce_ms
                .or_else(|| lsp_config.and_then(|config| config.debounce_ms))
                .unwrap_or(300),
            enable_code_lenses: enable_code_lenses
                .or_else(|| lsp_config.and_then(|config| config.enable_code_lenses))
                .unwrap_or(true),
            enable_hover: enable_hover
                .or_else(|| lsp_config.and_then(|config| config.enable_hover))
                .unwrap_or(true),
            diagnostic_severity: diagnostic_severity
                .or_else(|| lsp_config.and_then(|config| config.diagnostic_severity))
                .map_or(rivet_lsp::DiagnosticSeverity::Warning, Into::into),
        },
        build_context,
    ))
}

fn read_config_file(explicit: Option<PathBuf>) -> Result<Option<LoadedConfigFile>> {
    let Some(path) = find_config_path(explicit)? else {
        return Ok(None);
    };
    let contents =
        fs::read_to_string(&path).with_context(|| format!("failed to read {}", path.display()))?;
    let file = toml::from_str::<ConfigFile>(&contents)
        .with_context(|| format!("failed to parse {}", path.display()))?;
    Ok(Some(LoadedConfigFile { path, file }))
}

fn find_config_path(explicit: Option<PathBuf>) -> Result<Option<PathBuf>> {
    if explicit.is_some() {
        return Ok(explicit);
    }

    let mut current = env::current_dir()?;
    loop {
        let candidate = current.join("rivet.toml");
        if candidate.exists() {
            return Ok(Some(candidate));
        }
        if !current.pop() {
            break;
        }
    }

    let user_config = env::var_os("HOME")
        .map(PathBuf::from)
        .map(|home| home.join(".config/rivet/config.toml"));
    Ok(user_config.filter(|path| path.exists()))
}

impl PartialThresholds {
    fn apply_to(&self, target: &mut Thresholds) {
        if self.max_cyclomatic_complexity.is_some() {
            target.max_cyclomatic_complexity = self.max_cyclomatic_complexity;
        }
        if self.max_cognitive_complexity.is_some() {
            target.max_cognitive_complexity = self.max_cognitive_complexity;
        }
        if self.max_function_length.is_some() {
            target.max_function_length = self.max_function_length;
        }
        if self.max_parameter_count.is_some() {
            target.max_parameter_count = self.max_parameter_count;
        }
        if self.max_nesting_depth.is_some() {
            target.max_nesting_depth = self.max_nesting_depth;
        }
        if self.min_maintainability_index.is_some() {
            target.min_maintainability_index = self.min_maintainability_index;
        }
    }

    fn into_thresholds(self) -> Thresholds {
        Thresholds {
            max_cyclomatic_complexity: self.max_cyclomatic_complexity,
            max_cognitive_complexity: self.max_cognitive_complexity,
            max_function_length: self.max_function_length,
            max_parameter_count: self.max_parameter_count,
            max_nesting_depth: self.max_nesting_depth,
            min_maintainability_index: self.min_maintainability_index,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    fn unique_temp_dir(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rivet-cli-{name}-{suffix}"));
        fs::create_dir_all(&path).expect("temp dir");
        path
    }

    #[test]
    fn render_languages_json_includes_support_metadata() {
        let rendered = render_languages(OutputFormat::Json).expect("render json");
        assert!(rendered.contains("\"support_level\""));
        assert!(rendered.contains("\"parse_only\""));
        assert!(rendered.contains("\"full\""));
    }

    #[test]
    fn render_languages_text_includes_display_columns() {
        let rendered = render_languages(OutputFormat::Text).expect("render text");
        assert!(rendered.contains("Rust"));
        assert!(rendered.contains("full"));
        assert!(rendered.contains("parse_only"));
    }

    #[test]
    fn load_config_applies_cache_settings_from_file() {
        let root = unique_temp_dir("cache-config");
        let config_path = root.join("rivet.toml");
        fs::write(
            &config_path,
            r#"
[cache]
enabled = false
dir = "custom-cache"
mode = "read_only"
"#,
        )
        .expect("write config");

        let cli = Cli {
            command: None,
            language: None,
            format: OutputFormat::Text,
            output: None,
            config: Some(config_path),
        };

        let (_config, build_context) = load_config(&cli, None).expect("load config");
        assert!(!build_context.cache.enabled);
        assert_eq!(build_context.cache.dir, PathBuf::from("custom-cache"));
        assert_eq!(build_context.cache.mode, CacheMode::ReadOnly);

        fs::remove_dir_all(root).expect("cleanup");
    }
}

impl ThresholdOverrideFile {
    fn into_core_override(self) -> PathThresholdOverride {
        PathThresholdOverride {
            paths: self.paths,
            thresholds: self.thresholds.into_thresholds(),
        }
    }
}

impl PluginsConfigFile {
    fn into_core_plugin_config(self) -> PluginConfig {
        let mut plugin_config = PluginConfig::default();
        if let Some(enabled) = self.enabled {
            plugin_config.enabled = enabled;
        }
        plugin_config.discovery_paths = self.discovery_paths;
        plugin_config.entries = self
            .entries
            .into_iter()
            .map(PluginEntryFile::into_core_entry)
            .collect();
        if let Some(max_memory_pages) = self.max_memory_pages {
            plugin_config.max_memory_pages = max_memory_pages;
        }
        if let Some(timeout_ms) = self.timeout_ms {
            plugin_config.timeout_ms = timeout_ms;
        }
        plugin_config
    }
}

impl CacheConfigFile {
    fn into_runtime_cache_config(self) -> RuntimeCacheConfig {
        let mut cache_config = RuntimeCacheConfig::default();
        if let Some(enabled) = self.enabled {
            cache_config.enabled = enabled;
        }
        if let Some(dir) = self.dir {
            cache_config.dir = dir;
        }
        if let Some(mode) = self.mode {
            cache_config.mode = mode;
        }
        cache_config
    }
}

impl PluginEntryFile {
    fn into_core_entry(self) -> PluginEntryConfig {
        PluginEntryConfig {
            path: self.path,
            name: self.name,
            enabled: self.enabled.unwrap_or(true),
        }
    }
}

impl From<DiagnosticSeverityArg> for rivet_lsp::DiagnosticSeverity {
    fn from(value: DiagnosticSeverityArg) -> Self {
        match value {
            DiagnosticSeverityArg::Warning => Self::Warning,
            DiagnosticSeverityArg::Information => Self::Information,
        }
    }
}
