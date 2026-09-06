import { ensureSyntaxTree, syntaxTree } from "@codemirror/language";
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

function rangesFromTree(
  tree: ReturnType<typeof syntaxTree>,
  documentLength: number,
  highlightStyle: Highlighter,
): SyntaxHighlightRange[] {
  const ranges: SyntaxHighlightRange[] = [];
  highlightTree(tree, highlightStyle, (from, to, className) => {
    if (from >= to || from >= documentLength) return;
    ranges.push({
      from,
      to: Math.min(to, documentLength),
      className,
    });
  });

  return ranges;
}

/** Uses the editable CodeMirror view's current incremental syntax tree. */
export function getSyntaxHighlightRangesFromState(
  state: EditorState,
  highlightStyle: Highlighter,
): SyntaxHighlightRange[] {
  if (state.doc.length === 0) return [];
  return rangesFromTree(syntaxTree(state), state.doc.length, highlightStyle);
}

/**
 * Builds syntax-highlight ranges for renderers without a live EditorView.
 * Parsing is bounded so read-only views remain responsive for large snippets.
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
  return rangesFromTree(tree, state.doc.length, highlightStyle);
}
