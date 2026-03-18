#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    rivet_lsp::run_stdio(rivet_lsp::LspConfig::default()).await
}
