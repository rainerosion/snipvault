import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const expectedTag = process.env.SNIPVAULT_RELEASE_TAG;

function readText(relativePath) {
  return fs.readFileSync(path.join(repositoryRoot, relativePath), "utf8");
}

function readJson(relativePath) {
  return JSON.parse(readText(relativePath));
}

function readCargoPackageVersion() {
  const cargoToml = readText(path.join("src-tauri", "Cargo.toml"));
  const packageSection = cargoToml.match(
    /^\[package\]\s*$([\s\S]*?)(?=^\[|$(?![\s\S]))/m,
  )?.[1];
  const version = packageSection?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];

  if (!version)
    throw new Error(
      "Could not read [package].version from src-tauri/Cargo.toml",
    );
  return version;
}

function readCargoLockRootVersion() {
  const cargoLock = readText(path.join("src-tauri", "Cargo.lock"));
  const packageBlocks =
    cargoLock.match(
      /^\[\[package\]\]\s*$[\s\S]*?(?=^\[\[package\]\]|$(?![\s\S]))/gm,
    ) ?? [];
  const block = packageBlocks.find((candidate) =>
    /^name\s*=\s*"snipvault"\s*$/m.test(candidate),
  );
  const version = block?.match(/^version\s*=\s*"([^"]+)"\s*$/m)?.[1];
  if (!version)
    throw new Error(
      "Could not read snipvault version from src-tauri/Cargo.lock",
    );
  return version;
}

function readViteDefinedVersion() {
  const viteConfig = readText("vite.config.ts");
  if (!viteConfig.includes('import packageJson from "./package.json"')) {
    throw new Error(
      "vite.config.ts must derive VITE_APP_VERSION from package.json",
    );
  }
  if (
    !viteConfig.includes(
      '"import.meta.env.VITE_APP_VERSION": JSON.stringify(packageJson.version)',
    )
  ) {
    throw new Error(
      "vite.config.ts must define import.meta.env.VITE_APP_VERSION from packageJson.version",
    );
  }
  return readJson("package.json").version;
}

const versions = new Map([
  ["package.json", readJson("package.json").version],
  ["src-tauri/Cargo.toml", readCargoPackageVersion()],
  ["src-tauri/Cargo.lock", readCargoLockRootVersion()],
  [
    "src-tauri/tauri.conf.json",
    readJson(path.join("src-tauri", "tauri.conf.json")).version,
  ],
  ["vite.config.ts VITE_APP_VERSION", readViteDefinedVersion()],
]);

for (const [file, version] of versions) {
  if (typeof version !== "string" || version.length === 0) {
    throw new Error(`Missing version in ${file}`);
  }
}

const uniqueVersions = new Set(versions.values());
if (uniqueVersions.size !== 1) {
  console.error("Version consistency check failed:");
  for (const [file, version] of versions)
    console.error(`- ${file}: ${version}`);
  process.exitCode = 1;
} else {
  const version = [...uniqueVersions][0];
  const expectedVersionTag = `v${version}`;
  if (expectedTag && expectedTag !== expectedVersionTag) {
    console.error(
      `Release tag mismatch: expected ${expectedVersionTag}, got ${expectedTag}`,
    );
    process.exitCode = 1;
  } else {
    console.log(`Version consistency check passed: ${version}`);
  }
}
