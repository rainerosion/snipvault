import { lazy, Suspense } from "react";

const RevisionCodeView = lazy(() =>
  import("./RevisionCodeView").then((module) => ({ default: module.RevisionCodeView })),
);

interface LazyRevisionCodePreviewProps {
  content: string;
  language: string;
  theme: "dark" | "light";
  ariaLabel: string;
  loadingLabel: string;
}

/** Defers CodeMirror parser/highlighter imports until a live revision is inspected. */
export function LazyRevisionCodePreview({ loadingLabel, ...props }: LazyRevisionCodePreviewProps) {
  return (
    <Suspense fallback={<p className="revision-history-empty" role="status">{loadingLabel}</p>}>
      <RevisionCodeView {...props} />
    </Suspense>
  );
}
