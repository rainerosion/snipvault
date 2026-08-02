import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const repositoryRoot = path.resolve(
  path.dirname(fileURLToPath(import.meta.url)),
  "..",
);
const docsRoot = path.join(repositoryRoot, "docs");

function collectMarkdownFiles(directory) {
  if (!fs.existsSync(directory)) return [];

  return fs.readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return collectMarkdownFiles(entryPath);
    return entry.isFile() && entry.name.endsWith(".md") ? [entryPath] : [];
  });
}

const markdownFiles = [
  path.join(repositoryRoot, "README.md"),
  path.join(repositoryRoot, "CLAUDE.md"),
  ...collectMarkdownFiles(docsRoot),
].sort();

function withoutFencedCode(markdown) {
  let fence = null;

  return markdown
    .split(/\r?\n/)
    .map((line) => {
      const match = line.match(/^\s*(`{3,}|~{3,})/);
      if (match) {
        const marker = match[1][0];
        if (fence === null) fence = marker;
        else if (fence === marker) fence = null;
        return "";
      }
      return fence === null ? line : "";
    })
    .join("\n");
}

function githubSlug(heading) {
  return heading
    .toLowerCase()
    .trim()
    .replace(/<[^>]*>/g, "")
    .replace(/[`*_~]/g, "")
    .replace(/[^\p{Letter}\p{Number}\s_-]/gu, "")
    .replace(/\s+/g, "-");
}

const anchorCache = new Map();

function markdownAnchors(filePath) {
  if (anchorCache.has(filePath)) return anchorCache.get(filePath);

  const markdown = withoutFencedCode(fs.readFileSync(filePath, "utf8"));
  const counts = new Map();
  const anchors = new Set();

  for (const line of markdown.split("\n")) {
    const match = line.match(/^#{1,6}\s+(.+?)\s*#*\s*$/);
    if (!match) continue;

    const base = githubSlug(match[1]);
    const count = counts.get(base) ?? 0;
    counts.set(base, count + 1);
    anchors.add(count === 0 ? base : `${base}-${count}`);
  }

  anchorCache.set(filePath, anchors);
  return anchors;
}

function lineNumberAt(source, index) {
  return source.slice(0, index).split("\n").length;
}

function decodeLinkPart(value, sourceFile, lineNumber, errors) {
  try {
    return decodeURIComponent(value);
  } catch {
    errors.push(
      `${sourceFile}:${lineNumber}: invalid URL encoding in link: ${value}`,
    );
    return null;
  }
}

const errors = [];
let checkedLinks = 0;

for (const absoluteSourceFile of markdownFiles) {
  if (!fs.existsSync(absoluteSourceFile)) {
    errors.push(
      `${path.relative(repositoryRoot, absoluteSourceFile)}: file does not exist`,
    );
    continue;
  }

  const sourceFile = path
    .relative(repositoryRoot, absoluteSourceFile)
    .replaceAll("\\", "/");
  const markdown = withoutFencedCode(
    fs.readFileSync(absoluteSourceFile, "utf8"),
  );
  const targetPatterns = [
    /\]\(\s*(?:<([^>\n]+)>|([^\s)\n]+))/g,
    /^\s{0,3}\[[^\]]+\]:\s*(?:<([^>\n]+)>|([^\s]+))/gm,
  ];

  for (const pattern of targetPatterns) {
    for (const match of markdown.matchAll(pattern)) {
      const rawTarget = match[1] ?? match[2];
      if (!rawTarget || /^(?:[a-z][a-z\d+.-]*:|\/\/)/i.test(rawTarget))
        continue;

      checkedLinks += 1;
      const lineNumber = lineNumberAt(markdown, match.index ?? 0);
      const hashIndex = rawTarget.indexOf("#");
      const rawPath =
        hashIndex >= 0 ? rawTarget.slice(0, hashIndex) : rawTarget;
      const rawAnchor = hashIndex >= 0 ? rawTarget.slice(hashIndex + 1) : "";
      const pathWithoutQuery = rawPath.split("?", 1)[0];
      const decodedPath = decodeLinkPart(
        pathWithoutQuery,
        sourceFile,
        lineNumber,
        errors,
      );
      const decodedAnchor = decodeLinkPart(
        rawAnchor,
        sourceFile,
        lineNumber,
        errors,
      );
      if (decodedPath === null || decodedAnchor === null) continue;

      const destination = decodedPath
        ? path.resolve(path.dirname(absoluteSourceFile), decodedPath)
        : absoluteSourceFile;

      if (!fs.existsSync(destination)) {
        errors.push(
          `${sourceFile}:${lineNumber}: missing relative target: ${rawTarget}`,
        );
        continue;
      }

      if (decodedAnchor) {
        if (
          !fs.statSync(destination).isFile() ||
          path.extname(destination).toLowerCase() !== ".md"
        ) {
          errors.push(
            `${sourceFile}:${lineNumber}: anchor target is not Markdown: ${rawTarget}`,
          );
          continue;
        }

        if (!markdownAnchors(destination).has(decodedAnchor)) {
          errors.push(
            `${sourceFile}:${lineNumber}: missing Markdown anchor: ${rawTarget}`,
          );
        }
      }
    }
  }
}

if (errors.length > 0) {
  console.error(
    `Documentation link check failed with ${errors.length} error(s):`,
  );
  for (const error of errors) console.error(`- ${error}`);
  process.exitCode = 1;
} else {
  console.log(
    `Documentation link check passed: ${markdownFiles.length} Markdown files, ${checkedLinks} relative links.`,
  );
}
