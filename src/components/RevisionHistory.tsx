import { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type {
  RevisionComparison,
  RevisionContent,
  RevisionPage,
  RevisionSummary,
} from "../types";
import { localizeCommandError } from "../utils/commandErrors";
import { LazyRevisionCodePreview } from "./LazyRevisionCodePreview";
import { LazyRevisionDiffViewer } from "./LazyRevisionDiffViewer";

export interface RevisionHistoryTarget {
  snippet_id: string;
  current_revision_id: string;
  generation: number;
}

export type RevisionHistoryRestoreOutcome = {
  generation: number;
  status: "succeeded" | "cancelled" | "failed";
};

interface RevisionHistoryProps {
  target: RevisionHistoryTarget;
  theme: "dark" | "light";
  loadPage: (cursor: string | null) => Promise<RevisionPage>;
  loadRevision: (revisionId: string) => Promise<RevisionContent>;
  compare: (leftRevisionId: string, rightRevisionId: string) => Promise<RevisionComparison>;
  onRestore: (targetRevisionId: string) => Promise<void>;
  restoreOutcome: RevisionHistoryRestoreOutcome | null;
}

function formatRevisionTime(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function formatTimelineTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(date);
}

function revisionOptionLabel(
  revision: RevisionSummary,
  labels: { current: string; deleted: string; origin: (origin: string) => string },
): string {
  const state = revision.is_current_head
    ? labels.current
    : labels.origin(revision.origin);
  return `${formatRevisionTime(revision.revision_time)} · ${state}${revision.deleted ? ` · ${labels.deleted}` : ""}`;
}

function renderMetadata(
  content: RevisionContent | null,
  labels: { language: string; tags: string; favorite: string; yes: string; no: string },
) {
  if (!content || content.revision.deleted) return null;
  const snippet = content.snippet;
  if (!snippet) return null;
  return (
    <dl className="revision-history-meta">
      <div><dt>{labels.language}</dt><dd>{snippet.language}</dd></div>
      <div><dt>{labels.tags}</dt><dd>{snippet.tags.length ? snippet.tags.join(", ") : "—"}</dd></div>
      <div><dt>{labels.favorite}</dt><dd>{snippet.is_favorite ? labels.yes : labels.no}</dd></div>
    </dl>
  );
}

/** Native-window content for immutable revision inspection, comparison, and restore requests. */
export function RevisionHistory({
  target,
  theme,
  loadPage,
  loadRevision,
  compare,
  onRestore,
  restoreOutcome,
}: RevisionHistoryProps) {
  const { t } = useTranslation();
  const [items, setItems] = useState<RevisionSummary[]>([]);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState(target.current_revision_id);
  const [compareId, setCompareId] = useState<string | null>(null);
  const [preview, setPreview] = useState<RevisionContent | null>(null);
  const [comparison, setComparison] = useState<RevisionComparison | null>(null);
  const [loading, setLoading] = useState(true);
  const [loadingMore, setLoadingMore] = useState(false);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [comparisonLoading, setComparisonLoading] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [restoreFailed, setRestoreFailed] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [comparisonError, setComparisonError] = useState<unknown>(null);
  const previewRequestRef = useRef(0);
  const comparisonRequestRef = useRef(0);

  const metadataLabels = {
    language: t("snippet.revisionLanguage"),
    tags: t("snippet.revisionTags"),
    favorite: t("snippet.revisionFavorite"),
    yes: t("snippet.revisionYes"),
    no: t("snippet.revisionNo"),
  };

  const loadInitial = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const page = await loadPage(null);
      setItems(page.items);
      setNextCursor(page.next_cursor);
    } catch (cause) {
      setError(cause);
    } finally {
      setLoading(false);
    }
  }, [loadPage]);

  useEffect(() => {
    void loadInitial();
  }, [loadInitial]);

  useEffect(() => {
    const requestId = ++previewRequestRef.current;
    comparisonRequestRef.current += 1;
    setComparisonLoading(false);
    setPreviewLoading(true);
    setComparison(null);
    setComparisonError(null);
    setError(null);
    void loadRevision(selectedId)
      .then((content) => {
        if (requestId === previewRequestRef.current) setPreview(content);
      })
      .catch((cause) => {
        if (requestId === previewRequestRef.current) setError(cause);
      })
      .finally(() => {
        if (requestId === previewRequestRef.current) setPreviewLoading(false);
      });
  }, [loadRevision, selectedId]);

  useEffect(() => {
    if (!restoreOutcome || restoreOutcome.generation !== target.generation) return;
    if (restoreOutcome.status === "succeeded") return;
    setRestoring(false);
    setRestoreFailed(restoreOutcome.status === "failed");
  }, [restoreOutcome, target.generation]);

  const handleSelect = useCallback((revisionId: string) => {
    if (revisionId === selectedId) return;
    setSelectedId(revisionId);
    setComparison(null);
    setComparisonError(null);
  }, [selectedId]);

  const handleCompareTarget = useCallback((revisionId: string) => {
    comparisonRequestRef.current += 1;
    setComparisonLoading(false);
    setCompareId(revisionId || null);
    setComparison(null);
    setComparisonError(null);
  }, []);

  const handleExitComparison = useCallback(() => {
    comparisonRequestRef.current += 1;
    setComparisonLoading(false);
    setCompareId(null);
    setComparison(null);
    setComparisonError(null);
  }, []);

  const handleLoadMore = useCallback(async () => {
    if (!nextCursor || loadingMore) return;
    setLoadingMore(true);
    setError(null);
    try {
      const page = await loadPage(nextCursor);
      setItems((current) => [...current, ...page.items]);
      setNextCursor(page.next_cursor);
    } catch (cause) {
      setError(cause);
    } finally {
      setLoadingMore(false);
    }
  }, [loadPage, loadingMore, nextCursor]);

  const handleCompare = useCallback(async () => {
    if (!compareId || compareId === selectedId || comparisonLoading) return;
    const requestId = ++comparisonRequestRef.current;
    const leftRevisionId = compareId;
    const rightRevisionId = selectedId;
    setComparisonLoading(true);
    setComparisonError(null);
    try {
      const next = await compare(leftRevisionId, rightRevisionId);
      if (
        requestId === comparisonRequestRef.current
        && compareId === leftRevisionId
        && selectedId === rightRevisionId
      ) {
        setComparison(next);
      }
    } catch (cause) {
      if (requestId === comparisonRequestRef.current) setComparisonError(cause);
    } finally {
      if (requestId === comparisonRequestRef.current) setComparisonLoading(false);
    }
  }, [compare, compareId, comparisonLoading, selectedId]);

  const handleRestore = useCallback(async () => {
    if (!preview || preview.revision.deleted || preview.revision.is_current_head || restoring) return;
    setRestoring(true);
    setRestoreFailed(false);
    try {
      await onRestore(preview.revision.revision_id);
    } catch (cause) {
      setError(cause);
      setRestoring(false);
    }
  }, [onRestore, preview, restoring]);

  const showingComparison = comparison !== null;

  return (
    <main className="revision-history-window" aria-busy={loading || previewLoading || comparisonLoading || restoring}>
      <h1 className="sr-only">{t("snippet.history")}</h1>

      <div className="revision-history-body">
        <aside className="revision-history-timeline" aria-label={t("snippet.history")}>
          {loading ? (
            <p className="revision-history-empty" role="status">{t("snippet.historyLoading")}</p>
          ) : items.length === 0 ? (
            <p className="revision-history-empty">{t("snippet.historyEmpty")}</p>
          ) : (
            <ul className="revision-history-list">
              {items.map((revision) => {
                const isSelected = revision.revision_id === selectedId;
                return (
                  <li key={revision.revision_id}>
                    <button
                      type="button"
                      className={`revision-history-item ${isSelected ? "selected" : ""}`}
                      aria-current={isSelected ? "true" : undefined}
                      onClick={() => handleSelect(revision.revision_id)}
                    >
                      <span className="revision-history-item-time">{formatTimelineTime(revision.revision_time)}</span>
                      <span className="revision-history-item-detail">
                        {revision.is_current_head ? t("snippet.currentRevision") : t(`snippet.revisionOrigin.${revision.origin}`)}
                        {revision.deleted ? ` · ${t("snippet.deletedRevision")}` : ""}
                      </span>
                    </button>
                  </li>
                );
              })}
              {nextCursor && (
                <li>
                  <button type="button" className="revision-history-more" onClick={() => void handleLoadMore()} disabled={loadingMore}>
                    {loadingMore ? t("snippet.historyLoading") : t("snippet.historyLoadMore")}
                  </button>
                </li>
              )}
            </ul>
          )}
        </aside>

        <section className="revision-history-detail">
          {error ? (
            <p className="revision-history-error" role="alert">{localizeCommandError(error, t)}</p>
          ) : previewLoading || !preview ? (
            <p className="revision-history-empty" role="status">{t("snippet.historyLoading")}</p>
          ) : (
            <>
              <div className="revision-history-command-band">
                <div className="revision-history-selected-context">
                  <p className="revision-history-time">{formatRevisionTime(preview.revision.revision_time)}</p>
                  <p className="revision-history-origin">{preview.revision.is_current_head ? t("snippet.currentRevision") : t(`snippet.revisionOrigin.${preview.revision.origin}`)}</p>
                </div>
                <div className="revision-history-compare-control">
                  <label htmlFor="revision-compare-select">{t("snippet.compareRevision")}</label>
                  <select
                    id="revision-compare-select"
                    value={compareId ?? ""}
                    onChange={(event) => handleCompareTarget(event.target.value)}
                  >
                    <option value="">{t("snippet.compareChoose")}</option>
                    {items.filter((item) => item.revision_id !== selectedId).map((revision) => (
                      <option key={revision.revision_id} value={revision.revision_id}>
                        {revisionOptionLabel(revision, {
                          current: t("snippet.currentRevision"),
                          deleted: t("snippet.deletedRevision"),
                          origin: (origin) => t(`snippet.revisionOrigin.${origin}`),
                        })}
                      </option>
                    ))}
                  </select>
                  <button
                    type="button"
                    className="btn-copy"
                    onClick={() => void handleCompare()}
                    disabled={!compareId || comparisonLoading}
                  >
                    {comparisonLoading ? t("snippet.comparing") : t("snippet.compare")}
                  </button>
                </div>
                {!preview.revision.is_current_head && !preview.revision.deleted && (
                  <button type="button" className="btn-save" onClick={() => void handleRestore()} disabled={restoring}>
                    {restoring ? t("snippet.restoringRevision") : t("snippet.restoreRevision")}
                  </button>
                )}
              </div>

              <div className="revision-history-feedback">
                {restoreFailed && (
                  <p className="revision-history-error" role="alert">
                    {t("snippet.historyRestoreFailed")}
                  </p>
                )}
                {comparisonError !== null && (
                  <p className="revision-history-error" role="alert">
                    {localizeCommandError(comparisonError, t)}
                  </p>
                )}
              </div>

              <div className="revision-history-code-stage">
                {comparison && (
                  <div className="revision-history-comparison-stage">
                    <div className="revision-history-stage-heading">
                      <h2>{t("snippet.comparison")}</h2>
                      <button type="button" className="btn-copy" onClick={handleExitComparison}>
                        {t("snippet.exitComparison")}
                      </button>
                    </div>
                    <LazyRevisionDiffViewer
                      comparison={comparison}
                      theme={theme}
                      loadingLabel={t("snippet.comparing")}
                    />
                  </div>
                )}

                {!showingComparison && (preview.revision.deleted ? (
                  <div className="revision-history-tombstone">
                    <strong>{t("snippet.deletedRevision")}</strong>
                    <p>{t("snippet.deletedRevisionDescription", { time: preview.deleted_at ? formatRevisionTime(preview.deleted_at) : "—" })}</p>
                  </div>
                ) : preview.snippet ? (
                  <div className="revision-history-preview">
                    <div className="revision-history-preview-header">
                      <span>{preview.snippet.title || t("snippet.revisionPreview")}</span>
                    </div>
                    {renderMetadata(preview, metadataLabels)}
                    <LazyRevisionCodePreview
                      content={preview.snippet.content}
                      language={preview.snippet.language}
                      theme={theme}
                      ariaLabel={t("snippet.revisionPreview")}
                      loadingLabel={t("snippet.historyLoading")}
                    />
                  </div>
                ) : null)}
              </div>
            </>
          )}
        </section>
      </div>
    </main>
  );
}
