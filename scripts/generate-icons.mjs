import { spawnSync } from "node:child_process";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const sourceIcon = path.join(repositoryRoot, "assets", "app-icon.png");
const externalExports = [
  { relativePath: "assets/logo-1080.png", size: 1080 },
  {
    relativePath: "assets/microsoft-store/app-tile-icon-300.png",
    size: 300,
  },
];
const outputDir = path.join(repositoryRoot, "src-tauri", "icons");

if (!fs.existsSync(sourceIcon)) {
  throw new Error("Missing canonical icon source at assets/app-icon.png");
}

for (const { relativePath, size } of externalExports) {
  const output = path.join(repositoryRoot, relativePath);
  fs.mkdirSync(path.dirname(output), { recursive: true });
  await sharp(sourceIcon)
    .resize(size, size, { fit: "fill", kernel: sharp.kernel.lanczos3 })
    .png()
    .toFile(output);
}

fs.mkdirSync(outputDir, { recursive: true });
const sourceIconForTauri = "assets/app-icon.png";
const outputDirForTauri = "src-tauri/icons";
const result =
  process.platform === "win32"
    ? spawnSync(
        process.env.ComSpec ?? "cmd.exe",
        [
          "/d",
          "/s",
          "/c",
          `npx.cmd tauri icon ${sourceIconForTauri} --output ${outputDirForTauri}`,
        ],
        { cwd: repositoryRoot, stdio: "inherit" },
      )
    : spawnSync(
        "npx",
        ["tauri", "icon", sourceIconForTauri, "--output", outputDirForTauri],
        {
          cwd: repositoryRoot,
          stdio: "inherit",
        },
      );

if (result.error) throw result.error;
if (result.status !== 0) {
  process.exitCode = result.status ?? 1;
}
