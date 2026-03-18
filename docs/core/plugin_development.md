# Plugin Development

Rivet plugins are small WASM modules that receive per-function input as JSON and return one or more metric outputs as JSON.

The current workspace includes the SDK contract in [crates/rivet-plugin-sdk](/Users/yu/local/src/github/rivet/crates/rivet-plugin-sdk), the JSON schema in [plugin-interface.schema.json](/Users/yu/local/src/github/rivet/docs/core/plugin-interface.schema.json), and a minimal example plugin in [function_name_length.rs](/Users/yu/local/src/github/rivet/crates/rivet-plugin-sdk/example_plugins/function_name_length.rs).

## Input Contract

Plugins receive a JSON payload with these fields:

- `source`: function source text
- `function_name`: function or method name
- `language`: Rivet language id such as `rust` or `python`
- `sexp`: Tree-sitter S-expression for the function node
- `start_line`: 1-based start line
- `end_line`: 1-based end line

The formal schema is in [plugin-interface.schema.json](/Users/yu/local/src/github/rivet/docs/core/plugin-interface.schema.json).

## Output Contract

Plugins return a JSON array of metric outputs:

```json
[
  {
    "metric_id": "function_name_length",
    "display_name": "Function Name Length",
    "value": 12
  }
]
```

`value` may be an integer, a float, or a nested object of metric values.

## Example Plugin

The example plugin measures the length of a function name:

```rust
use rivet_plugin_sdk::{AnalyzeOutput, MetricValue, handle_plugin};

#[extism_pdk::plugin_fn]
pub fn analyze(input: String) -> extism_pdk::FnResult<String> {
    handle_plugin(&input, |payload| {
        Ok(vec![AnalyzeOutput {
            metric_id: "function_name_length".to_string(),
            display_name: "Function Name Length".to_string(),
            value: MetricValue::Integer(i64::try_from(payload.function_name.len()).unwrap_or(0)),
        }])
    })
}
```

## Build

The example plugin targets `wasm32-unknown-unknown`:

```bash
cargo build \
  -p rivet-plugin-sdk \
  --features example-wasm \
  --example function_name_length \
  --target wasm32-unknown-unknown \
  --release
```

## Current Status

- The SDK contract and example plugin are in place.
- The core plugin runtime in `rivet-core` is still the next implementation step.
- Once the runtime is added, plugin discovery and `rivet.toml` loading can be wired to this contract without changing the example plugin surface.
