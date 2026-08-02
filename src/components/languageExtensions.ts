import type { Extension } from "@codemirror/state";
import { StreamLanguage } from "@codemirror/language";
import { cpp } from "@codemirror/lang-cpp";
import { css } from "@codemirror/lang-css";
import { go } from "@codemirror/lang-go";
import { html } from "@codemirror/lang-html";
import { java } from "@codemirror/lang-java";
import { javascript } from "@codemirror/lang-javascript";
import { json } from "@codemirror/lang-json";
import { markdown } from "@codemirror/lang-markdown";
import { php } from "@codemirror/lang-php";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";
import { yaml } from "@codemirror/lang-yaml";
import { csharp } from "@replit/codemirror-lang-csharp";
import { elixir } from "codemirror-lang-elixir";
import { kotlin, scala } from "@codemirror/legacy-modes/mode/clike";
import { dockerFile } from "@codemirror/legacy-modes/mode/dockerfile";
import { lua } from "@codemirror/legacy-modes/mode/lua";
import { r } from "@codemirror/legacy-modes/mode/r";
import { ruby } from "@codemirror/legacy-modes/mode/ruby";
import { shell } from "@codemirror/legacy-modes/mode/shell";
import { swift } from "@codemirror/legacy-modes/mode/swift";
import { toml } from "@codemirror/legacy-modes/mode/toml";
import type { LanguageId } from "../utils/languages";

export type LanguageSupportKind =
  | "parser-backed"
  | "stream-highlighted"
  | "plaintext-fallback";

export const LANGUAGE_SUPPORT: Record<LanguageId, LanguageSupportKind> = {
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
};

const streamLanguages: Partial<
  Record<LanguageId, ReturnType<typeof StreamLanguage.define>>
> = {
  ruby: StreamLanguage.define(ruby),
  swift: StreamLanguage.define(swift),
  kotlin: StreamLanguage.define(kotlin),
  bash: StreamLanguage.define(shell),
  dockerfile: StreamLanguage.define(dockerFile),
  toml: StreamLanguage.define(toml),
  lua: StreamLanguage.define(lua),
  r: StreamLanguage.define(r),
  scala: StreamLanguage.define(scala),
};

export function getLanguageExtensions(language: LanguageId): Extension {
  const streamLanguage = streamLanguages[language];
  if (streamLanguage) return streamLanguage;

  switch (language) {
    case "javascript":
      return javascript({ jsx: false, typescript: false });
    case "typescript":
      return javascript({ jsx: false, typescript: true });
    case "jsx":
      return javascript({ jsx: true, typescript: false });
    case "tsx":
      return javascript({ jsx: true, typescript: true });
    case "python":
      return python();
    case "rust":
      return rust();
    case "go":
      return go();
    case "java":
      return java();
    case "cpp":
    case "c":
      return cpp();
    case "csharp":
      return csharp();
    case "php":
      return php();
    case "sql":
      return sql();
    case "html":
      return html();
    case "xml":
      return xml();
    case "json":
      return json();
    case "css":
      return css();
    case "markdown":
      return markdown();
    case "yaml":
      return yaml();
    case "elixir":
      return elixir();
    case "plaintext":
      return [];
    case "ruby":
    case "swift":
    case "kotlin":
    case "bash":
    case "dockerfile":
    case "toml":
    case "lua":
    case "r":
    case "scala":
      return [];
    default: {
      const exhaustive: never = language;
      return exhaustive;
    }
  }
}
