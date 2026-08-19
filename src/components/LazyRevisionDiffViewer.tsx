import { lazy, Suspense } from "react";
import type { RevisionComparison } from "../types";

const RevisionDiffViewer = lazy(() =>
  import("./RevisionDiffViewer").then((module) => ({ default: module.RevisionDiffViewer })),
);

interface LazyRevisionDiffViewerProps {
  comparison: RevisionComparison;
  theme: "dark" | "light";
  loadingLabel: string;
}

/** Defers line-diff and parser code until the user starts a comparison. */
export function LazyRevisionDiffViewer({ loadingLabel, ...props }: LazyRevisionDiffViewerProps) {
  return (
    <Suspense fallback={<p className="revision-history-empty" role="status">{loadingLabel}</p>}>
      <RevisionDiffViewer {...props} />
    </Suspense>
  );
}
