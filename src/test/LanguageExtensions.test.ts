import { describe, expect, it } from "vitest";
import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { LANGUAGES, type LanguageId } from "../utils/languages";
import {
  getLanguageExtensions,
  LANGUAGE_SUPPORT,
  type LanguageSupportKind,
} from "../components/languageExtensions";

const EXPECTED_SUPPORT = {
  plaintext: "plaintext-fallback",
  javascript: "parser-backed",
  typescript: "parser-backed",
  jsx: "parser-backed",
  tsx: "parser-backed",
  python: "parser-backed",
  rust: "parser-backed",
  go: "parser-backed",
  java: "parser-backed",
  cpp: "parser-backed",
  c: "parser-backed",
  csharp: "parser-backed",
  php: "parser-backed",
  ruby: "stream-highlighted",
  swift: "stream-highlighted",
  kotlin: "stream-highlighted",
  sql: "parser-backed",
  html: "parser-backed",
  css: "parser-backed",
  json: "parser-backed",
  yaml: "parser-backed",
  xml: "parser-backed",
  markdown: "parser-backed",
  bash: "stream-highlighted",
  dockerfile: "stream-highlighted",
  toml: "stream-highlighted",
  lua: "stream-highlighted",
  r: "stream-highlighted",
  scala: "stream-highlighted",
  elixir: "parser-backed",
} as const satisfies Record<LanguageId, LanguageSupportKind>;

function syntaxTreeName(language: LanguageId, document: string) {
  const state = EditorState.create({
    doc: document,
    extensions: getLanguageExtensions(language),
  });
  return ensureSyntaxTree(state, state.doc.length, 1_000)?.type.name;
}

describe("editor language extensions", () => {
  it("exhaustively classifies every selectable language", () => {
    expect(LANGUAGE_SUPPORT).toEqual(EXPECTED_SUPPORT);
    expect(Object.keys(LANGUAGE_SUPPORT).sort()).toEqual(
      LANGUAGES.map(({ id }) => id).sort(),
    );
  });

  it("uses the correct parser families for HTML, C#, and Go", () => {
    expect(syntaxTreeName("html", "<main><h1>Hi</h1></main>")).toBe("Document");
    expect(
      syntaxTreeName("csharp", "public class Vault { static void Main() {} }"),
    ).toBe("Program");
    expect(syntaxTreeName("go", "package main\nfunc main() {}")).toBe(
      "SourceFile",
    );
  });
});
