import { Component, lazy } from "react";
import type { ComponentProps } from "react";
import type { SnippetEditor } from "./SnippetEditor";

type SnippetEditorModule = typeof import("./SnippetEditor");

type LazySnippetEditorProps = ComponentProps<typeof SnippetEditor> & {
  attempt: number;
};

function loadSnippetEditorModule(attempt: number): Promise<SnippetEditorModule> {
  if (import.meta.env.DEV && attempt > 0) {
    return import(
      /* @vite-ignore */ `/src/components/SnippetEditor.tsx?editor-load-attempt=${attempt}`,
    ) as Promise<SnippetEditorModule>;
  }

  return import("./SnippetEditor");
}

function createLazySnippetEditor(attempt: number) {
  return lazy(() =>
    loadSnippetEditorModule(attempt).then((module) => ({
      default: module.SnippetEditor,
    })),
  );
}

const lazySnippetEditors = new Map<number, ReturnType<typeof createLazySnippetEditor>>([
  [0, createLazySnippetEditor(0)],
]);

function getLazySnippetEditor(attempt: number) {
  const existing = lazySnippetEditors.get(attempt);
  if (existing) return existing;

  const editor = createLazySnippetEditor(attempt);
  lazySnippetEditors.set(attempt, editor);
  return editor;
}

export class LazySnippetEditor extends Component<LazySnippetEditorProps> {
  private readonly Editor: ReturnType<typeof createLazySnippetEditor>;

  constructor(props: LazySnippetEditorProps) {
    super(props);
    this.Editor = getLazySnippetEditor(props.attempt);
  }

  render() {
    const { attempt: _attempt, ...props } = this.props;
    void _attempt;
    return <this.Editor {...props} />;
  }
}
