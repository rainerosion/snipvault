import { ensureSyntaxTree } from "@codemirror/language";
import { EditorState } from "@codemirror/state";
import { highlightTree, type Highlighter } from "@lezer/highlight";
import type { LanguageId } from "../utils/languages";
import { getLanguageExtensions, LANGUAGE_SUPPORT } from "./languageExtensions";

const SYNTAX_PARSE_BUDGET_MS = 50;

export interface SyntaxHighlightRange {
  from: number;
  to: number;
  className: string;
}

export function normalizeLanguage(language: string): LanguageId {
  return language in LANGUAGE_SUPPORT ? (language as LanguageId) : "plaintext";
}

/**
 * Builds syntax-highlight ranges with the same language extension and
 * HighlightStyle used by the editable CodeMirror view. Parsing is bounded so
 * the Canvas codeglance remains a progressive enhancement for large snippets.
 */
export function getSyntaxHighlightRanges(
  content: string,
  language: string,
  highlightStyle: Highlighter,
): SyntaxHighlightRange[] {
  if (!content) return [];

  const state = EditorState.create({
    doc: content,
    extensions: getLanguageExtensions(normalizeLanguage(language)),
  });
  const tree = ensureSyntaxTree(
    state,
    state.doc.length,
    SYNTAX_PARSE_BUDGET_MS,
  );

  if (!tree) return [];

  const ranges: SyntaxHighlightRange[] = [];
  highlightTree(tree, highlightStyle, (from, to, className) => {
    if (from >= to || from >= state.doc.length) return;
    ranges.push({
      from,
      to: Math.min(to, state.doc.length),
      className,
    });
  });

  return ranges;
}
