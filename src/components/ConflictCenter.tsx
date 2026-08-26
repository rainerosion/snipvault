import { useCallback, useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type {
  ConflictPage,
  RevisionComparison,
  SyncConflictResolution,
  SyncConflictReview,
  SyncConflictSummary,
} from "../types";
import { localizeCommandError } from "../utils/commandErrors";
import { ModalSurface } from "./ModalSurface";
import { LazyRevisionDiffViewer } from "./LazyRevisionDiffViewer";

interface ConflictCenterProps {
  theme: "dark" | "light";
  onClose: () => void;
  onResolve: (review: SyncConflictReview, action: SyncConflictResolution) => Promise<boolean>;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

type ComparisonKind = "preservedIncoming" | "preservedAncestor" | "incomingAncestor";

export function ConflictCenter({ theme, onClose, onResolve }: ConflictCenterProps) {
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  const [items, setItems] = useState<SyncConflictSummary[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [review, setReview] = useState<SyncConflictReview | null>(null);
  const [loading, setLoading] = useState(true);
  const [reviewLoading, setReviewLoading] = useState(false);
  const [resolving, setResolving] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [comparisonKind, setComparisonKind] = useState<ComparisonKind>("preservedIncoming");

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const page = await invoke<ConflictPage>("list_sync_conflicts", { state: "open", limit: 50 });
      setItems(page.items);
      setSelectedId((current) => current ?? page.items[0]?.conflict_id ?? null);
    } catch (cause) {
      setError(cause);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => { void load(); }, [load]);

  useEffect(() => {
    if (!selectedId) {
      setReview(null);
      return;
    }
    let active = true;
    setReviewLoading(true);
    setError(null);
    void invoke<SyncConflictReview>("get_sync_conflict_review", { conflictId: selectedId })
      .then((next) => {
        if (active) setReview(next);
      })
      .catch((cause) => {
        if (active) setError(cause);
      })
      .finally(() => {
        if (active) setReviewLoading(false);
      });
    return () => { active = false; };
  }, [selectedId]);

  const comparison = (): RevisionComparison | null => {
    if (!review) return null;
    if (comparisonKind === "preservedAncestor" && review.common_ancestor) {
      return { left: review.common_ancestor, right: review.preserved_local };
    }
    if (comparisonKind === "incomingAncestor" && review.common_ancestor) {
      return { left: review.common_ancestor, right: review.incoming };
    }
    return { left: review.preserved_local, right: review.incoming };
  };

  const resolve = async (action: SyncConflictResolution) => {
    if (!review || resolving) return;
    setResolving(true);
    try {
      const handled = await onResolve(review, action);
      if (handled) await load();
    } finally {
      setResolving(false);
    }
  };

  const canApply = review && !review.source_deleted && review.source_current_revision_id === review.incoming.revision.revision_id;
  const canRecreate = review && review.source_deleted && review.source_current_revision_id === review.incoming.revision.revision_id;

  return (
    <div className="conflict-center-overlay">
      <ModalSurface className="conflict-center-dialog" labelledBy="conflict-center-title" describedBy="conflict-center-description" initialFocusRef={closeRef} onEscape={onClose}>
        <header className="conflict-center-header">
          <div>
            <p className="conflict-center-kicker">{t("conflicts.kicker")}</p>
            <h2 id="conflict-center-title">{t("conflicts.title")}</h2>
            <p id="conflict-center-description">{t("conflicts.description")}</p>
          </div>
          <button ref={closeRef} type="button" className="settings-close" onClick={onClose} aria-label={t("settings.close")}>×</button>
        </header>
        <div className="conflict-center-body" aria-busy={loading || reviewLoading || resolving}>
          <aside className="conflict-center-list" aria-label={t("conflicts.listLabel")}>
            {loading ? <p role="status">{t("conflicts.loading")}</p> : items.length === 0 ? <p>{t("conflicts.empty")}</p> : items.map((item) => (
              <button key={item.conflict_id} type="button" className={item.conflict_id === selectedId ? "selected" : ""} onClick={() => { setSelectedId(item.conflict_id); setComparisonKind("preservedIncoming"); }}>
                <strong>{item.source_deleted ? t("conflicts.deletedSource") : t("conflicts.liveSource")}</strong>
                <span>{formatDate(item.detected_at)}</span>
              </button>
            ))}
          </aside>
          <section className="conflict-center-detail">
            {error !== null ? <p className="conflict-center-error" role="alert">{localizeCommandError(error, t)}</p> : reviewLoading ? <p role="status">{t("conflicts.reviewLoading")}</p> : review && (
              <>
                <p className="conflict-center-explanation">{t("conflicts.remoteWon")}</p>
                {review.source_current_revision_id !== review.incoming.revision.revision_id && <p className="conflict-center-warning">{t("conflicts.superseded")}</p>}
                <div className="conflict-center-compare" role="group" aria-label={t("conflicts.comparison")}>
                  <button type="button" aria-pressed={comparisonKind === "preservedIncoming"} onClick={() => setComparisonKind("preservedIncoming")}>{t("conflicts.preservedVsIncoming")}</button>
                  {review.common_ancestor && <>
                    <button type="button" aria-pressed={comparisonKind === "preservedAncestor"} onClick={() => setComparisonKind("preservedAncestor")}>{t("conflicts.preservedVsAncestor")}</button>
                    <button type="button" aria-pressed={comparisonKind === "incomingAncestor"} onClick={() => setComparisonKind("incomingAncestor")}>{t("conflicts.incomingVsAncestor")}</button>
                  </>}
                </div>
                {comparison() && <LazyRevisionDiffViewer comparison={comparison()!} theme={theme} loadingLabel={t("conflicts.comparing")} />}
                <div className="conflict-center-actions">
                  {review.source_current_revision_id !== review.incoming.revision.revision_id ? (
                    <button type="button" className="btn-copy" disabled={resolving} onClick={() => void resolve("review_superseded")}>{t("conflicts.markReviewed")}</button>
                  ) : review.source_deleted ? (
                    <>
                      <button type="button" className="btn-copy" disabled={resolving} onClick={() => void resolve("keep_incoming")}>{t("conflicts.keepDeletion")}</button>
                      <button type="button" className="btn-save" disabled={resolving || !canRecreate} onClick={() => void resolve("recreate_preserved")}>{t("conflicts.createNew")}</button>
                    </>
                  ) : (
                    <>
                      <button type="button" className="btn-copy" disabled={resolving} onClick={() => void resolve("keep_incoming")}>{t("conflicts.keepIncoming")}</button>
                      <button type="button" className="btn-save" disabled={resolving || !canApply} onClick={() => void resolve("apply_preserved")}>{t("conflicts.applyPreserved")}</button>
                    </>
                  )}
                </div>
              </>
            )}
          </section>
        </div>
      </ModalSurface>
    </div>
  );
}
