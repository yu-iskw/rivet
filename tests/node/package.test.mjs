import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(scriptDir, "..", "..");
const packageDir = resolve(repoRoot, "crates", "rivet-node");
const packageJsonPath = resolve(packageDir, "package.json");
const npmCacheDir = resolve(repoRoot, ".tmp", "npm-cache");

mkdirSync(npmCacheDir, { recursive: true });

const packageJson = JSON.parse(readFileSync(packageJsonPath, "utf8"));
assert.equal(packageJson.name, "@rivet-rs/node");
assert.equal(packageJson.main, "./index.js");
assert.equal(packageJson.types, "index.d.ts");
assert.deepEqual(packageJson.exports, {
  ".": {
    types: "./index.d.ts",
    default: "./index.js",
    require: "./index.js",
  },
});
assert.ok(Array.isArray(packageJson.files));
assert.ok(packageJson.files.includes("index.d.ts"));
assert.ok(packageJson.files.includes("index.js"));
assert.equal(typeof packageJson.scripts.build, "string");
assert.equal(typeof packageJson.scripts.test, "string");
assert.equal(typeof packageJson.scripts["validate:package"], "string");

const typeDefinitionPath = resolve(packageDir, packageJson.types);
assert.ok(existsSync(typeDefinitionPath), "index.d.ts must exist");
assert.ok(
  existsSync(resolve(packageDir, packageJson.main)),
  "main entrypoint target must exist",
);
assert.ok(
  existsSync(resolve(packageDir, packageJson.exports["."].default)),
  "exports.default target must exist",
);
const nativeAddonName = `rivet_node.${process.platform}-${process.arch}.node`;
assert.ok(
  packageJson.files.includes("rivet_node.*.node"),
  "package files must include the native addon glob",
);

const typeDefinitions = readFileSync(typeDefinitionPath, "utf8");
for (const snippet of [
  "export interface AnalyzerOptions",
  "export interface FileAnalysis",
  "export interface ProjectAnalysis",
  "export declare class JsAnalyzer",
  "analyzeSource(",
  "analyzeDirectory(path: string, language?: string): Promise<ProjectAnalysis>;",
  "checkThresholds(analysis: ProjectAnalysis): ThresholdResult;",
  "supportedLanguages(): string[];",
]) {
  assert.ok(
    typeDefinitions.includes(snippet),
    `expected ${packageJson.types} to include ${snippet}`,
  );
}

const packOutput = execFileSync("npm", ["pack", "--dry-run", "--json"], {
  cwd: packageDir,
  encoding: "utf8",
  env: { ...process.env, npm_config_cache: npmCacheDir },
});
const [packInfo] = JSON.parse(packOutput);
assert.ok(
  Array.isArray(packInfo.files),
  "npm pack --dry-run must list packaged files",
);
assert.ok(
  packInfo.files.some((file) => file.path.endsWith("package.json")),
  "packed files must include package.json",
);
assert.ok(
  packInfo.files.some((file) => file.path.endsWith("index.d.ts")),
  "packed files must include index.d.ts",
);
assert.ok(
  packInfo.files.some((file) => file.path === "index.js"),
  "packed files must include the runtime entrypoint",
);
assert.ok(
  packInfo.files.some((file) => file.path === nativeAddonName),
  "packed files must include the native addon artifact",
);
