use anyhow::Result;

#[derive(Debug, Clone, Copy)]
pub struct LspConfig {
    pub analyze_on_change: bool,
}

pub fn run_stdio(config: LspConfig) -> Result<()> {
    println!(
        "{{\"server\":\"rivet-lsp\",\"status\":\"ready\",\"analyze_on_change\":{}}}",
        config.analyze_on_change
    );
    Ok(())
}
