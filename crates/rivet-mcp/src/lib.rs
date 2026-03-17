use anyhow::Result;
use rivet_core::{Analyzer, AnalyzerConfig};

pub fn run_stdio() -> Result<()> {
    let analyzer = Analyzer::new(AnalyzerConfig::default())?;
    println!(
        "{{\"server\":\"rivet-mcp\",\"status\":\"ready\",\"supported_languages\":{}}}",
        serde_json::to_string(
            &analyzer
                .supported_languages()
                .into_iter()
                .map(rivet_core::Language::as_str)
                .collect::<Vec<_>>()
        )?
    );
    Ok(())
}
