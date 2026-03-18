import assert from "node:assert/strict";
import { createRequire } from "node:module";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const packageDir = resolve(scriptDir, "..", "..", "crates", "rivet-node");
const require = createRequire(import.meta.url);
const addon = require(resolve(packageDir, "index.js"));

assert.equal(typeof addon.JsAnalyzer, "function");

const analyzer = new addon.JsAnalyzer();
const analysis = analyzer.analyzeSource(
  "fn sample(value: i32) -> i32 { if value > 0 { value } else { 0 } }",
  "rust",
);

assert.equal(analysis.language, "rust");
assert.equal(analysis.functions[0].name, "sample");
