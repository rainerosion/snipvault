import {
  autocompletion,
  snippetCompletion,
  type Completion,
  type CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from "@codemirror/autocomplete";
import { EditorState, type Extension } from "@codemirror/state";
import type { LanguageId } from "../utils/languages";

const MAX_COMPLETIONS = 160;
const MAX_DOCUMENT_SCAN_CHARS = 120_000;
const MAX_DOCUMENT_WORDS = 100;
const MAX_VAULT_TERMS = 80;
const IDENTIFIER_START = /[A-Za-z_$\p{L}\p{Nl}]/u;
const IDENTIFIER = /[\w$\-\p{L}\p{Nl}\p{Mn}\p{Mc}\p{Nd}\p{Pc}]/u;
const TOKEN_BEFORE = /[\w$\-\p{L}\p{Nl}\p{Mn}\p{Mc}\p{Nd}\p{Pc}]*/u;
const WORD = /[A-Za-z_$\p{L}\p{Nl}][\w$\-\p{L}\p{Nl}\p{Mn}\p{Mc}\p{Nd}\p{Pc}]{1,}/gu;
const LANGUAGE_KEYWORDS: Record<LanguageId, readonly string[]> = {
  plaintext: ["TODO", "NOTE", "FIXME"],
  javascript: ["const", "let", "var", "function", "return", "if", "else", "for", "while", "async", "await", "import", "export", "class", "new", "try", "catch", "console.log"],
  typescript: ["const", "let", "interface", "type", "enum", "function", "return", "if", "else", "for", "async", "await", "import", "export", "class", "implements", "console.log"],
  jsx: ["const", "let", "function", "return", "if", "else", "import", "export", "class", "useState", "useEffect", "console.log"],
  tsx: ["const", "let", "interface", "type", "function", "return", "if", "else", "import", "export", "class", "useState", "useEffect", "console.log"],
  python: ["def", "class", "return", "if", "elif", "else", "for", "while", "in", "import", "from", "as", "try", "except", "with", "async", "await", "print", "self"],
  rust: ["fn", "let", "mut", "struct", "enum", "impl", "trait", "pub", "use", "mod", "match", "if", "else", "for", "while", "loop", "async", "await", "return", "println!"],
  go: ["package", "import", "func", "var", "const", "type", "struct", "interface", "return", "if", "else", "for", "range", "go", "defer", "chan", "select", "fmt.Println"],
  java: ["package", "import", "public", "private", "protected", "class", "interface", "extends", "implements", "static", "final", "void", "new", "return", "if", "else", "for", "while", "try", "catch", "System.out.println"],
  cpp: ["#include", "using", "namespace", "class", "struct", "template", "public", "private", "const", "auto", "void", "int", "return", "if", "else", "for", "while", "try", "catch", "std::cout"],
  c: ["#include", "#define", "struct", "typedef", "const", "static", "void", "int", "char", "return", "if", "else", "for", "while", "switch", "printf"],
  csharp: ["using", "namespace", "class", "interface", "public", "private", "protected", "static", "async", "await", "void", "var", "new", "return", "if", "else", "foreach", "try", "catch", "Console.WriteLine"],
  php: ["<?php", "namespace", "use", "class", "interface", "public", "private", "protected", "function", "return", "if", "else", "foreach", "while", "try", "catch", "echo"],
  ruby: ["class", "module", "def", "end", "do", "if", "elsif", "else", "unless", "case", "when", "while", "until", "require", "yield", "puts"],
  swift: ["import", "class", "struct", "enum", "protocol", "extension", "let", "var", "func", "return", "if", "else", "for", "while", "guard", "switch", "case", "print"],
  kotlin: ["package", "import", "class", "object", "interface", "data", "fun", "val", "var", "return", "if", "else", "when", "for", "while", "suspend", "println"],
  sql: ["SELECT", "FROM", "WHERE", "JOIN", "LEFT JOIN", "RIGHT JOIN", "INNER JOIN", "GROUP BY", "ORDER BY", "HAVING", "INSERT INTO", "UPDATE", "DELETE FROM", "CREATE TABLE", "ALTER TABLE", "LIMIT"],
  html: ["<!doctype html>", "html", "head", "body", "main", "section", "div", "span", "h1", "p", "a", "button", "input", "form", "script", "style"],
  css: ["display", "position", "relative", "absolute", "flex", "grid", "color", "background", "margin", "padding", "border", "width", "height", "font-size", "media"],
  json: ["true", "false", "null"],
  yaml: ["true", "false", "null", "services", "version", "environment", "volumes", "ports"],
  xml: ["xml", "root", "item", "name", "value", "id"],
  markdown: ["#", "##", "###", "-", "*", "[link]", "```", "**bold**", "_italic_"],
  bash: ["#!/usr/bin/env bash", "if", "then", "elif", "else", "fi", "for", "in", "do", "done", "while", "case", "function", "echo", "export", "source"],
  dockerfile: ["FROM", "RUN", "COPY", "ADD", "WORKDIR", "ENV", "EXPOSE", "CMD", "ENTRYPOINT", "USER", "VOLUME"],
  toml: ["true", "false", "package", "dependencies", "dev-dependencies", "features", "workspace"],
  lua: ["local", "function", "end", "if", "then", "elseif", "else", "for", "in", "while", "repeat", "until", "require", "return", "print"],
  r: ["library", "require", "function", "return", "if", "else", "for", "while", "repeat", "next", "break", "print", "data.frame"],
  scala: ["package", "import", "class", "object", "trait", "case class", "def", "val", "var", "if", "else", "match", "for", "yield", "println"],
  elixir: ["defmodule", "def", "defp", "do", "end", "fn", "case", "cond", "if", "unless", "for", "in", "alias", "import", "use", "IO.puts"],
};

interface SnippetTemplate {
  label: string;
  template: string;
}

/** Local-only suggestion catalog; language packages can add richer contextual providers. */
const COMMON_SNIPPETS: Partial<Record<LanguageId, readonly SnippetTemplate[]>> = {
  javascript: [
    { label: "function", template: "function ${name}(${}) {\n\t${}\n}" },
    { label: "arrow function", template: "const ${name} = (${}) => {\n\t${}\n};" },
    { label: "try / catch", template: "try {\n\t${}\n} catch (${error}) {\n\t${}\n}" },
  ],
  typescript: [
    { label: "interface", template: "interface ${Name} {\n\t${}\n}" },
    { label: "type", template: "type ${Name} = ${};" },
    { label: "function", template: "function ${name}(${}) : ${void} {\n\t${}\n}" },
  ],
  jsx: [{ label: "component", template: "function ${Component}() {\n\treturn <${div}>${}</${div}>;\n}" }],
  tsx: [{ label: "component", template: "function ${Component}(): JSX.Element {\n\treturn <${div}>${}</${div}>;\n}" }],
  python: [
    { label: "function", template: "def ${name}(${}) -> ${None}:\n    ${pass}" },
    { label: "main guard", template: "if __name__ == \"__main__\":\n    ${}" },
  ],
  rust: [
    { label: "main", template: "fn main() {\n    ${}\n}" },
    { label: "match", template: "match ${value} {\n    ${_} => ${}\n}" },
  ],
  go: [
    { label: "main", template: "func main() {\n\t${}\n}" },
    { label: "error check", template: "if err != nil {\n\treturn ${err}\n}" },
  ],
  java: [{ label: "main", template: "public static void main(String[] args) {\n    ${}\n}" }],
  cpp: [{ label: "main", template: "int main() {\n    ${}\n    return 0;\n}" }],
  c: [{ label: "main", template: "int main(void) {\n    ${}\n    return 0;\n}" }],
  csharp: [{ label: "main", template: "public static void Main() {\n    ${}\n}" }],
  php: [{ label: "function", template: "function ${name}(${}) {\n    ${}\n}" }],
  ruby: [{ label: "method", template: "def ${name}\n  ${}\nend" }],
  swift: [{ label: "function", template: "func ${name}(${}) {\n    ${}\n}" }],
  kotlin: [{ label: "main", template: "fun main() {\n    ${}\n}" }],
  sql: [{ label: "select", template: "SELECT ${columns}\nFROM ${table}\nWHERE ${condition};" }],
  html: [{ label: "element", template: "<${div}>${}</${div}>" }],
  css: [{ label: "media query", template: "@media (width >= 768px) {\n  ${}\n}" }],
  bash: [{ label: "if", template: "if [[ ${condition} ]]; then\n  ${}\nfi" }],
  dockerfile: [{ label: "base image", template: "FROM ${image}:${tag}\nWORKDIR /app" }],
  lua: [{ label: "function", template: "function ${name}()\n  ${}\nend" }],
  r: [{ label: "function", template: "${name} <- function(${}) {\n  ${}\n}" }],
  scala: [{ label: "function", template: "def ${name}(${}): ${Unit} = {\n  ${}\n}" }],
  elixir: [{ label: "module", template: "defmodule ${Name} do\n  ${}\nend" }],
};

function completion(label: string, type: string, boost: number): Completion {
  return { label, type, boost };
}

function completionKey(candidate: Completion): string {
  return `${candidate.label}::${candidate.detail ?? ""}::${candidate.type ?? ""}::${typeof candidate.apply === "string" ? candidate.apply : ""}`.toLocaleLowerCase();
}

function documentWords(
  state: EditorState,
  position: number,
  currentPrefix: string,
): Completion[] {
  const halfWindow = Math.floor(MAX_DOCUMENT_SCAN_CHARS / 2);
  const start = Math.max(0, Math.min(position - halfWindow, state.doc.length - MAX_DOCUMENT_SCAN_CHARS));
  const end = Math.min(state.doc.length, start + MAX_DOCUMENT_SCAN_CHARS);
  const source = state.doc.sliceString(start, end);
  const prefix = currentPrefix.toLocaleLowerCase();
  const words = new Set<string>();
  WORD.lastIndex = 0;
  let match: RegExpExecArray | null;
  while ((match = WORD.exec(source)) && words.size < MAX_DOCUMENT_WORDS) {
    const word = match[0];
    if (word.toLocaleLowerCase().startsWith(prefix)) words.add(word);
  }
  return [...words].map((word) => completion(word, "variable", 10));
}

function vaultCompletions(terms: readonly string[], currentPrefix: string): Completion[] {
  const prefix = currentPrefix.toLocaleLowerCase();
  const seen = new Set<string>();
  return terms
    .slice(0, MAX_VAULT_TERMS)
    .filter((term) => {
      const value = term.trim();
      const key = value.toLocaleLowerCase();
      if (!value || seen.has(key) || !key.startsWith(prefix)) return false;
      seen.add(key);
      return true;
    })
    .map((term) => completion(term.trim(), "text", -5));
}

function localOptions(
  language: LanguageId,
  terms: readonly string[],
  state: EditorState,
  position: number,
  currentPrefix: string,
): Completion[] {
  const options: Completion[] = [];
  const seen = new Set<string>();
  const add = (candidate: Completion) => {
    const key = completionKey(candidate);
    if (!seen.has(key) && options.length < MAX_COMPLETIONS) {
      seen.add(key);
      options.push(candidate);
    }
  };

  for (const keyword of LANGUAGE_KEYWORDS[language]) {
    add(completion(keyword, "keyword", 25));
  }
  for (const entry of COMMON_SNIPPETS[language] ?? []) {
    add(snippetCompletion(entry.template, completion(entry.label, "snippet", 20)));
  }
  for (const candidate of documentWords(state, position, currentPrefix)) add(candidate);
  for (const candidate of vaultCompletions(terms, currentPrefix)) add(candidate);
  return options;
}

export function createLocalCompletionSource(
  language: LanguageId,
  terms: readonly string[] = [],
): CompletionSource {
  return (context: CompletionContext): CompletionResult | null => {
    const token = context.matchBefore(TOKEN_BEFORE);
    const currentPrefix = token?.text ?? "";
    if (!context.explicit && (!token || !IDENTIFIER.test(currentPrefix))) return null;

    return {
      from: token?.from ?? context.pos,
      to: token?.to ?? context.pos,
      options: localOptions(language, terms, context.state, context.pos, currentPrefix),
      validFor: TOKEN_BEFORE,
    };
  };
}

export function localCompletionExtension(
  language: LanguageId,
  terms: readonly string[] = [],
): Extension {
  return [
    EditorState.languageData.of(() => [
      { autocomplete: createLocalCompletionSource(language, terms) },
    ]),
    autocompletion({
      defaultKeymap: true,
      maxRenderedOptions: MAX_COMPLETIONS,
      tooltipClass: () => "snipvault-completion-tooltip",
    }),
  ];
}
