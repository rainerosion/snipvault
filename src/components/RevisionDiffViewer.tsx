import { useCallback, useMemo, useRef, useState, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import type { RevisionComparison, RevisionContent } from "../types";
import { buildLineDiff, sourceLines, type DiffRow } from "./lineDiff";
import { RevisionCodeView } from "./RevisionCodeView";

interface RevisionDiffViewerProps {
  comparison: RevisionComparison;
  theme: "dark" | "light";
}

function formatRevisionTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function revisionLabel(content: RevisionContent, currentLabel: string, deletedLabel: string): string {
  if (content.revision.deleted) return deletedLabel;
  return content.revision.is_current_head ? currentLabel : formatRevisionTime(content.revision.revision_time);
}

function tombstonePane(
  content: RevisionContent,
  label: string,
  description: string,
  side: "left" | "right",
) {
  return (
    <section
      id={`revision-diff-${side}-pane`}
      className={`revision-diff-pane revision-diff-pane-${side} revision-diff-tombstone`}
      aria-label={label}
    >
      <header className="revision-diff-pane-header">
        <strong>{label}</strong>
        <span>{formatRevisionTime(content.revision.revision_time)}</span>
      </header>
      <p>{description}</p>
    </section>
  );
}

export function RevisionDiffViewer({ comparison, theme }: RevisionDiffViewerProps) {
  const { t } = useTranslation();
  const leftScrollRef = useRef<HTMLDivElement>(null);
  const rightScrollRef = useRef<HTMLDivElement>(null);
  const [activePane, setActivePane] = useState<"left" | "right">("right");
  const syncingScrollRef = useRef(false);
  const left = comparison.left;
  const right = comparison.right;
  const leftSnippet = left.snippet;
  const rightSnippet = right.snippet;
  const diff = useMemo(
    () => leftSnippet && rightSnippet
      ? buildLineDiff(leftSnippet.content, rightSnippet.content)
      : null,
    [leftSnippet, rightSnippet],
  );

  const rows = useMemo<DiffRow[] | undefined>(() => {
    if (!leftSnippet || !rightSnippet || !diff) return undefined;
    if (diff.status === "ready") return diff.rows;
    if (diff.status === "identical") {
      return sourceLines(leftSnippet.content).map((line) => ({
        kind: "equal",
        left: line,
        right: line,
      }));
    }
    return undefined;
  }, [diff, leftSnippet, rightSnippet]);

  const handleScroll = useCallback((side: "left" | "right") => (event: UIEvent<HTMLDivElement>) => {
    if (syncingScrollRef.current || !rows) return;
    const source = event.currentTarget;
    const target = side === "left" ? rightScrollRef.current : leftScrollRef.current;
    if (!target) return;
    syncingScrollRef.current = true;
    target.scrollTop = source.scrollTop;
    requestAnimationFrame(() => {
      syncingScrollRef.current = false;
    });
  }, [rows]);

  if (!leftSnippet && !rightSnippet) {
    return (
      <div className="revision-diff-empty" role="status">
        {t("snippet.compareBothDeleted")}
      </div>
    );
  }

  const leftLabel = revisionLabel(left, t("snippet.currentRevision"), t("snippet.compareDeleted"));
  const rightLabel = revisionLabel(right, t("snippet.currentRevision"), t("snippet.compareDeleted"));
  const summary = diff?.status === "ready"
    ? t("snippet.compareSummary", { additions: diff.additions, deletions: diff.deletions })
    : diff?.status === "identical"
      ? t("snippet.compareIdentical", { count: diff.lineCount })
      : diff?.status === "limited"
        ? t(`snippet.compareLimited.${diff.reason}`)
        : null;

  return (
    <div className="revision-diff-workbench">
      <div className="revision-diff-toolbar">
        <div>
          <h2>{t("snippet.comparison")}</h2>
          {summary && <p className="revision-diff-summary" role="status">{summary}</p>}
        </div>
        <div className="revision-diff-pane-switch" role="group" aria-label={t("snippet.comparison")}>
          <button
            type="button"
            aria-pressed={activePane === "left"}
            aria-controls="revision-diff-left-pane"
            onClick={() => setActivePane("left")}
          >
            {t("snippet.compareBaseline")}
          </button>
          <button
            type="button"
            aria-pressed={activePane === "right"}
            aria-controls="revision-diff-right-pane"
            onClick={() => setActivePane("right")}
          >
            {t("snippet.compareAfter")}
          </button>
        </div>
      </div>
      <div className="revision-diff-viewport">
        <div className={`revision-diff-grid revision-diff-grid-${activePane}`}>
          {leftSnippet ? (
            <section id="revision-diff-left-pane" className="revision-diff-pane revision-diff-pane-left" aria-labelledby="revision-diff-left-title">
              <header className="revision-diff-pane-header">
                <div>
                  <strong id="revision-diff-left-title">{t("snippet.compareBaseline")}</strong>
                  <span>{leftLabel}</span>
                </div>
                <small>{leftSnippet.language}</small>
              </header>
              <RevisionCodeView
                content={leftSnippet.content}
                language={leftSnippet.language}
                theme={theme}
                ariaLabel={t("snippet.comparePaneLabel", { side: t("snippet.compareBaseline") })}
                rows={rows}
                side="left"
                scrollRef={leftScrollRef}
                onScroll={handleScroll("left")}
              />
            </section>
          ) : tombstonePane(left, t("snippet.compareBaseline"), t("snippet.compareDeletedDescription"), "left")}

          {rightSnippet ? (
            <section id="revision-diff-right-pane" className="revision-diff-pane revision-diff-pane-right" aria-labelledby="revision-diff-right-title">
              <header className="revision-diff-pane-header">
                <div>
                  <strong id="revision-diff-right-title">{t("snippet.compareAfter")}</strong>
                  <span>{rightLabel}</span>
                </div>
                <small>{rightSnippet.language}</small>
              </header>
              <RevisionCodeView
                content={rightSnippet.content}
                language={rightSnippet.language}
                theme={theme}
                ariaLabel={t("snippet.comparePaneLabel", { side: t("snippet.compareAfter") })}
                rows={rows}
                side="right"
                scrollRef={rightScrollRef}
                onScroll={handleScroll("right")}
              />
            </section>
          ) : tombstonePane(right, t("snippet.compareAfter"), t("snippet.compareDeletedDescription"), "right")}
        </div>
      </div>
    </div>
  );
}
