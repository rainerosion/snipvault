import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import sharp from "sharp";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);

const iconDir = path.join(repositoryRoot, "src-tauri", "icons");
const requiredPngs = new Map([
  ["32x32.png", [32, 32]],
  ["64x64.png", [64, 64]],
  ["128x128.png", [128, 128]],
  ["128x128@2x.png", [256, 256]],
  ["icon.png", [512, 512]],
]);

function read(relativePath) {
  const file = path.join(repositoryRoot, relativePath);
  if (!fs.existsSync(file))
    throw new Error(`Missing required icon file: ${relativePath}`);
  return fs.readFileSync(file);
}

function assertHeader(relativePath, expectedHex) {
  const bytes = read(relativePath);
  const actual = bytes.subarray(0, expectedHex.length / 2).toString("hex");
  if (actual !== expectedHex) {
    throw new Error(
      `${relativePath} has invalid magic header ${actual}; expected ${expectedHex}`,
    );
  }
}

async function assertPng(relativePath, width, height) {
  assertHeader(relativePath, "89504e470d0a1a0a");
  const metadata = await sharp(
    path.join(repositoryRoot, relativePath),
  ).metadata();
  if (
    metadata.format !== "png" ||
    metadata.width !== width ||
    metadata.height !== height
  ) {
    throw new Error(
      `${relativePath} must be a ${width}x${height} PNG; got ${metadata.width}x${metadata.height} ${metadata.format}`,
    );
  }
}

for (const duplicate of [
  "gen-icon.cjs",
  "scripts/src-tauri",
  "src-tauri/icons/generate-icons.cjs",
  "public/icon-32.png",
  "public/icon-128.png",
]) {
  if (fs.existsSync(path.join(repositoryRoot, duplicate))) {
    throw new Error(`Legacy duplicate icon artifact remains: ${duplicate}`);
  }
}

await assertPng("assets/app-icon.png", 512, 512);
for (const [file, [width, height]] of requiredPngs) {
  await assertPng(path.join("src-tauri", "icons", file), width, height);
}
assertHeader("src-tauri/icons/icon.ico", "00000100");
assertHeader("src-tauri/icons/icon.icns", "69636e73");

const configuredIcons = JSON.parse(
  fs.readFileSync(
    path.join(repositoryRoot, "src-tauri", "tauri.conf.json"),
    "utf8",
  ),
).bundle.icon;
for (const icon of configuredIcons) {
  if (!fs.existsSync(path.join(iconDir, path.basename(icon)))) {
    throw new Error(`tauri.conf.json references missing icon: ${icon}`);
  }
}

console.log(
  "Icon check passed: canonical source and generated Tauri icons are valid.",
);
