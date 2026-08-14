import { Component, type ErrorInfo, type ReactNode } from "react";

interface SnippetEditorLoadBoundaryProps {
  children: ReactNode;
  developmentHint?: string;
  onRetry: () => void;
  retryLabel: string;
  title: string;
}

interface SnippetEditorLoadBoundaryState {
  hasError: boolean;
}

export class SnippetEditorLoadBoundary extends Component<
  SnippetEditorLoadBoundaryProps,
  SnippetEditorLoadBoundaryState
> {
  state: SnippetEditorLoadBoundaryState = { hasError: false };

  static getDerivedStateFromError(): SnippetEditorLoadBoundaryState {
    return { hasError: true };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Snippet editor could not be loaded or rendered.", error, errorInfo);
  }

  render() {
    const { children, developmentHint, onRetry, retryLabel, title } = this.props;

    if (!this.state.hasError) return children;

    return (
      <div className="editor-empty editor-load-error" role="alert">
        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1" aria-hidden="true">
          <path d="M12 9v4" />
          <path d="M12 17h.01" />
          <path d="M10.29 3.86 1.82 18a2 2 0 0 0 1.71 3h16.94a2 2 0 0 0 1.71-3L13.71 3.86a2 2 0 0 0-3.42 0Z" />
        </svg>
        <p>{title}</p>
        {developmentHint && <p className="hint">{developmentHint}</p>}
        <button type="button" className="snippet-retry-btn" onClick={onRetry}>
          {retryLabel}
        </button>
      </div>
    );
  }
}
