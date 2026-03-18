# Editor Setup

Rivet ships an LSP server binary, `rivet-lsp`, which can be integrated with editors directly.

For performance targets and benchmark commands, see [performance.md](/Users/yu/local/src/github/rivet/docs/core/performance.md).

## VS Code

A minimal VS Code client stub lives in [editors/vscode/package.json](/Users/yu/local/src/github/rivet/editors/vscode/package.json) and [editors/vscode/src/extension.js](/Users/yu/local/src/github/rivet/editors/vscode/src/extension.js).

Example `settings.json`:

```json
{
  "rivet.serverPath": "rivet-lsp",
  "rivet.analyzeOnChange": true,
  "rivet.codeLens.enable": true,
  "rivet.hover.enable": true,
  "rivet.thresholds.maxCyclomaticComplexity": 15,
  "rivet.thresholds.maxCognitiveComplexity": 15
}
```

Verification checklist:

- Open a supported file and confirm diagnostics appear without running a manual command.
- Hover a function declaration and confirm complexity metrics come from cached analysis.
- Confirm code lenses appear above functions when `rivet.codeLens.enable` is true.
- Change a threshold in settings and confirm diagnostics refresh after the config update.
- Confirm hover and code lens continue to read from cached analysis rather than re-running analysis on each request.

## Neovim

Example `nvim-lspconfig` setup:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")

if not configs.rivet then
  configs.rivet = {
    default_config = {
      cmd = { "rivet-lsp" },
      filetypes = { "rust", "python", "typescript", "javascript" },
      root_dir = lspconfig.util.root_pattern("rivet.toml", ".git"),
      settings = {},
    },
  }
end

lspconfig.rivet.setup({})
```

Verification checklist:

- Open a supported file and confirm `:LspInfo` shows `rivet-lsp` attached.
- Confirm publish diagnostics updates after a save or, when enabled, after an edit debounce.
- Run `vim.lsp.buf.hover()` over a function and confirm cached metric details are shown.

## Helix

Example `languages.toml`:

```toml
[[language]]
name = "rust"
language-servers = ["rust-analyzer", "rivet"]

[language-server.rivet]
command = "rivet-lsp"
```

Verification checklist:

- Open a Rust or Python file and confirm Rivet diagnostics render inline.
- Save the buffer after changing thresholds or code and confirm diagnostics refresh.

## Zed

Example `settings.json`:

```json
{
  "lsp": {
    "rivet": {
      "binary": {
        "path": "rivet-lsp"
      }
    }
  },
  "languages": {
    "Rust": { "language_servers": ["rust-analyzer", "rivet"] },
    "Python": { "language_servers": ["pyright", "rivet"] },
    "TypeScript": {
      "language_servers": ["typescript-language-server", "rivet"]
    }
  }
}
```

Verification checklist:

- Confirm `rivet-lsp` is launched for configured languages.
- Hover a function and verify metric details are displayed.
- Confirm code lenses and diagnostics disappear after closing the file.
