import { copyFileSync, existsSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const packageDir = dirname(fileURLToPath(import.meta.url));
const targetDir = resolve(
  packageDir,
  process.env.CARGO_TARGET_DIR ?? "../../target",
  process.env.RIVET_NODE_PROFILE ?? "release",
);

const sourceLibraryName =
  process.platform === "win32"
    ? "rivet_node.dll"
    : process.platform === "darwin"
      ? "librivet_node.dylib"
      : "librivet_node.so";
const sourceLibraryPath = resolve(targetDir, sourceLibraryName);
const packagedAddonPath = resolve(
  packageDir,
  `rivet_node.${process.platform}-${process.arch}.node`,
);

if (!existsSync(sourceLibraryPath)) {
  throw new Error(`missing compiled native library: ${sourceLibraryPath}`);
}

copyFileSync(sourceLibraryPath, packagedAddonPath);
