import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import type { LocalSnapshot } from "../types";
import { localizeCommandError } from "../utils/commandErrors";
import { ModalSurface } from "./ModalSurface";

interface RestoreWizardProps {
  snapshots: LocalSnapshot[];
  loading: boolean;
  onRefresh: () => Promise<void>;
  onCreateSnapshot: () => Promise<void>;
  onRestore: (snapshotId: string) => Promise<boolean>;
  onOpenFolder: () => Promise<void>;
  onClose: () => void;
}

function formatDate(value: string): string {
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function formatByteCount(value: number): string {
  if (value < 1024 * 1024) return `${Math.max(1, Math.round(value / 1024))} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}

export function RestoreWizard({
  snapshots,
  loading,
  onRefresh,
  onCreateSnapshot,
  onRestore,
  onOpenFolder,
  onClose,
}: RestoreWizardProps) {
  const { t } = useTranslation();
  const closeRef = useRef<HTMLButtonElement>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [creating, setCreating] = useState(false);
  const [restoring, setRestoring] = useState(false);
  const [error, setError] = useState<unknown>(null);

  const available = snapshots.filter((snapshot) => snapshot.unavailable_at === null);
  const selected = available.find((snapshot) => snapshot.id === selectedId) ?? null;

  useEffect(() => {
    if (!selectedId && available[0]) setSelectedId(available[0].id);
  }, [available, selectedId]);

  const handleCreate = async () => {
    if (creating || restoring) return;
    setCreating(true);
    setError(null);
    try {
      await onCreateSnapshot();
      await onRefresh();
    } catch (cause) {
      setError(cause);
    } finally {
      setCreating(false);
    }
  };

  const handleRestore = async () => {
    if (!selected || restoring || creating) return;
    setRestoring(true);
    setError(null);
    try {
      const restored = await onRestore(selected.id);
      if (!restored) {
        setRestoring(false);
        return;
      }
    } catch (cause) {
      setError(cause);
      setRestoring(false);
    }
  };

  return (
    <div className="restore-wizard-overlay">
      <ModalSurface
        className="restore-wizard-dialog"
        labelledBy="restore-wizard-title"
        describedBy="restore-wizard-description"
        initialFocusRef={closeRef}
        onEscape={onClose}
      >
        <header className="restore-wizard-header">
          <div>
            <p className="restore-wizard-kicker">{t("snapshots.kicker")}</p>
            <h2 id="restore-wizard-title">{t("snapshots.title")}</h2>
            <p id="restore-wizard-description">{t("snapshots.description")}</p>
          </div>
          <button ref={closeRef} type="button" className="settings-close" onClick={onClose} aria-label={t("settings.close")}>×</button>
        </header>

        <div className="restore-wizard-body" aria-busy={loading || creating || restoring}>
          <section className="restore-wizard-list-section">
            <div className="restore-wizard-actions">
              <button type="button" className="btn-copy" onClick={() => void handleCreate()} disabled={creating || restoring}>
                {creating ? t("snapshots.creating") : t("snapshots.createNow")}
              </button>
              <button type="button" className="about-link" onClick={() => void onOpenFolder().catch(setError)}>
                {t("snapshots.openFolder")}
              </button>
            </div>
            {loading ? (
              <p className="restore-wizard-empty" role="status">{t("snapshots.loading")}</p>
            ) : snapshots.length === 0 ? (
              <p className="restore-wizard-empty">{t("snapshots.empty")}</p>
            ) : (
              <div className="restore-wizard-list" role="list">
                {snapshots.map((snapshot) => {
                  const unavailable = snapshot.unavailable_at !== null;
                  return (
                    <button
                      key={snapshot.id}
                      type="button"
                      role="listitem"
                      disabled={unavailable || restoring}
                      className={`restore-wizard-item ${snapshot.id === selectedId ? "selected" : ""}`}
                      onClick={() => setSelectedId(snapshot.id)}
                    >
                      <span>{formatDate(snapshot.created_at)}</span>
                      <span>{unavailable ? t("snapshots.unavailable") : t("snapshots.snapshotInfo", { count: snapshot.snippet_count, size: formatByteCount(snapshot.byte_count) })}</span>
                    </button>
                  );
                })}
              </div>
            )}
          </section>

          <section className="restore-wizard-detail">
            {error !== null && (
              <p className="restore-wizard-error" role="alert">
                {localizeCommandError(error, t)}
              </p>
            )}
            <h3>{t("snapshots.restoreTitle")}</h3>
            <p>{t("snapshots.restoreScope")}</p>
            <ul>
              <li>{t("snapshots.scopeDatabase")}</li>
              <li>{t("snapshots.scopeExcluded")}</li>
              <li>{t("snapshots.scopeManualSync")}</li>
            </ul>
            {selected ? (
              <>
                <dl className="restore-wizard-meta">
                  <div><dt>{t("snapshots.createdAt")}</dt><dd>{formatDate(selected.created_at)}</dd></div>
                  <div><dt>{t("snapshots.verifiedAt")}</dt><dd>{formatDate(selected.verified_at)}</dd></div>
                  <div><dt>{t("snapshots.snippetCount")}</dt><dd>{selected.snippet_count}</dd></div>
                  <div><dt>{t("snapshots.snapshotSize")}</dt><dd>{formatByteCount(selected.byte_count)}</dd></div>
                </dl>
                <p className="restore-wizard-warning">{t("snapshots.restoreWarning")}</p>
                <button type="button" className="btn-save" onClick={() => void handleRestore()} disabled={restoring || creating}>
                  {restoring ? t("snapshots.restoring") : t("snapshots.restore")}
                </button>
              </>
            ) : (
              <p className="restore-wizard-empty">{t("snapshots.selectSnapshot")}</p>
            )}
          </section>
        </div>
      </ModalSurface>
    </div>
  );
}
