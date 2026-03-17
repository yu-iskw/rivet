#![allow(clippy::missing_const_for_fn)]

use std::{
    env, fs,
    path::{Path, PathBuf},
    str::FromStr,
};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use ignore::WalkBuilder;
use rivet_core::{
    Analyzer, AnalyzerConfig, FileInput, Language, ProjectAnalysis, Thresholds,
    output::{to_csv, to_json, to_sarif, to_text},
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
        #[arg(long, default_value_t = true)]
        analyze_on_change: bool,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum OutputFormat {
    Json,
    Text,
    Csv,
    Sarif,
}

#[derive(Debug, Default, serde::Deserialize)]
struct ConfigFile {
    thresholds: Option<Thresholds>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let command = cli.command.clone().unwrap_or_else(|| Command::Analyze {
        path: PathBuf::from("."),
    });

    match command {
        Command::Analyze { path } => {
            let analyzer = Analyzer::new(load_config(&cli, None)?)?;
            let analysis = analyze_path(&analyzer, &path, cli.language.as_deref())?;
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
            let overrides = Thresholds {
                max_cyclomatic_complexity: max_cc,
                max_cognitive_complexity: max_cognitive,
                max_function_length: max_length,
                max_parameter_count: max_params,
                max_nesting_depth: max_nesting,
                ..Thresholds::default()
            };
            let analyzer = Analyzer::new(load_config(&cli, Some(overrides))?)?;
            let analysis = analyze_path(&analyzer, &path, cli.language.as_deref())?;
            emit_output(&cli, &analysis)?;
            let result = analyzer.check_thresholds(&analysis);
            if !warning_only && !result.passed {
                std::process::exit(1);
            }
            Ok(())
        }
        Command::Languages => {
            let analyzer = Analyzer::new(load_config(&cli, None)?)?;
            for language in analyzer.supported_languages() {
                println!("{}", language.as_str());
            }
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
        Command::Serve => rivet_mcp::run_stdio(),
        Command::Lsp { analyze_on_change } => {
            rivet_lsp::run_stdio(rivet_lsp::LspConfig { analyze_on_change })
        }
    }
}

fn analyze_path(
    analyzer: &Analyzer,
    path: &Path,
    language_override: Option<&str>,
) -> Result<ProjectAnalysis> {
    let files = collect_files(path, language_override)?;
    analyzer.analyze_files(&files).map_err(Into::into)
}

fn collect_files(path: &Path, language_override: Option<&str>) -> Result<Vec<FileInput>> {
    if path.is_file() {
        return Ok(vec![build_file_input(path, language_override)?]);
    }

    let mut files = Vec::new();
    for entry in WalkBuilder::new(path).standard_filters(true).build() {
        let entry = entry?;
        if entry
            .file_type()
            .is_some_and(|file_type| file_type.is_file())
        {
            let candidate = entry.into_path();
            if let Ok(file) = build_file_input(&candidate, language_override) {
                files.push(file);
            }
        }
    }
    Ok(files)
}

fn build_file_input(path: &Path, language_override: Option<&str>) -> Result<FileInput> {
    let source = fs::read(path).with_context(|| format!("failed to read {}", path.display()))?;
    let language = match language_override {
        Some(value) => Language::from_str(value)?,
        None => Language::from_path(path)?,
    };
    Ok(FileInput {
        file_path: Some(path.to_path_buf()),
        language,
        source,
    })
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

fn load_config(cli: &Cli, threshold_overrides: Option<Thresholds>) -> Result<AnalyzerConfig> {
    let mut config = AnalyzerConfig::default();

    if let Some(path) = find_config_path(cli.config.clone())? {
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let file = toml::from_str::<ConfigFile>(&contents)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        if let Some(thresholds) = file.thresholds {
            config.thresholds = thresholds;
        }
    }

    if let Ok(value) = env::var("RIVET_MAX_CC") {
        config.thresholds.max_cyclomatic_complexity = value.parse().ok();
    }
    if let Ok(value) = env::var("RIVET_MAX_COGNITIVE") {
        config.thresholds.max_cognitive_complexity = value.parse().ok();
    }

    if let Some(overrides) = threshold_overrides {
        apply_overrides(&mut config.thresholds, &overrides);
    }

    Ok(config)
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

fn apply_overrides(target: &mut Thresholds, overrides: &Thresholds) {
    if overrides.max_cyclomatic_complexity.is_some() {
        target.max_cyclomatic_complexity = overrides.max_cyclomatic_complexity;
    }
    if overrides.max_cognitive_complexity.is_some() {
        target.max_cognitive_complexity = overrides.max_cognitive_complexity;
    }
    if overrides.max_function_length.is_some() {
        target.max_function_length = overrides.max_function_length;
    }
    if overrides.max_parameter_count.is_some() {
        target.max_parameter_count = overrides.max_parameter_count;
    }
    if overrides.max_nesting_depth.is_some() {
        target.max_nesting_depth = overrides.max_nesting_depth;
    }
}
