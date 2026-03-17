fn main() -> anyhow::Result<()> {
    rivet_lsp::run_stdio(rivet_lsp::LspConfig {
        analyze_on_change: true,
    })
}
