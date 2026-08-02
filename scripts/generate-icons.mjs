import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const sourceIcon = path.join(repositoryRoot, "assets", "app-icon.png");
const outputDir = path.join(repositoryRoot, "src-tauri", "icons");

if (!fs.existsSync(sourceIcon)) {
  throw new Error("Missing canonical icon source at assets/app-icon.png");
}

fs.mkdirSync(outputDir, { recursive: true });
const result = spawnSync(
  process.platform === "win32" ? "npx.cmd" : "npx",
  ["tauri", "icon", sourceIcon, "--output", outputDir],
  { cwd: repositoryRoot, stdio: "inherit" },
);

if (result.error) throw result.error;
if (result.status !== 0) {
  process.exitCode = result.status ?? 1;
}
