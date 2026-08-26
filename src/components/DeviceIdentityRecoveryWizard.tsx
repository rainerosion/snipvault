import { useEffect, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import type { DeviceIdentityRotation, DeviceIdentityStatus } from "../types";
import { localizeCommandError } from "../utils/commandErrors";
import { ModalSurface } from "./ModalSurface";
import { Dialog, type DialogHandle } from "./Dialog";

interface DeviceIdentityRecoveryWizardProps {
  onClose: () => void;
}

function formatDate(value: string | null): string {
  if (!value) return "—";
  const date = new Date(value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

export function DeviceIdentityRecoveryWizard({ onClose }: DeviceIdentityRecoveryWizardProps) {
  const { t } = useTranslation();
  const dialogRef = useRef<DialogHandle>(null);
  const closeRef = useRef<HTMLButtonElement>(null);
  const [status, setStatus] = useState<DeviceIdentityStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [rotating, setRotating] = useState(false);
  const [error, setError] = useState<unknown>(null);
  const [completedAt, setCompletedAt] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    void invoke<DeviceIdentityStatus>("get_device_identity_status")
      .then((next) => {
        if (active) setStatus(next);
      })
      .catch((cause) => {
        if (active) setError(cause);
      })
      .finally(() => {
        if (active) setLoading(false);
      });
    return () => {
      active = false;
    };
  }, []);

  const rotate = async () => {
    if (rotating || completedAt) return;
    if (await dialogRef.current?.confirm(
      t("identityRecovery.confirmRotation"),
      "identityRecovery.confirmTitle",
      { confirmLabel: "identityRecovery.rotate" },
    ) !== true) return;

    setRotating(true);
    setError(null);
    try {
      const result = await invoke<DeviceIdentityRotation>("rotate_device_identity");
      setCompletedAt(result.rotated_at);
      setStatus((previous) => previous && { ...previous, last_rotated_at: result.rotated_at });
    } catch (cause) {
      setError(cause);
    } finally {
      setRotating(false);
    }
  };

  return (
    <div className="identity-recovery-overlay">
      <ModalSurface
        className="identity-recovery-dialog"
        labelledBy="identity-recovery-title"
        describedBy="identity-recovery-description"
        initialFocusRef={closeRef}
        onEscape={onClose}
      >
        <Dialog ref={dialogRef} />
        <header className="identity-recovery-header">
          <div>
            <p className="identity-recovery-kicker">{t("identityRecovery.kicker")}</p>
            <h2 id="identity-recovery-title">{t("identityRecovery.title")}</h2>
            <p id="identity-recovery-description">{t("identityRecovery.description")}</p>
          </div>
          <button ref={closeRef} type="button" className="settings-close" onClick={onClose} aria-label={t("settings.close")}>×</button>
        </header>

        <div className="identity-recovery-body" aria-busy={loading || rotating}>
          {error !== null && <p className="identity-recovery-error" role="alert">{localizeCommandError(error, t)}</p>}
          {completedAt ? (
            <div className="identity-recovery-complete" role="status">
              <strong>{t("identityRecovery.completed")}</strong>
              <p>{t("identityRecovery.completedDescription", { time: formatDate(completedAt) })}</p>
            </div>
          ) : (
            <>
              <div className="identity-recovery-warning">
                <p>{t("identityRecovery.eligibility")}</p>
                <ul>
                  <li>{t("identityRecovery.futureOnly")}</li>
                  <li>{t("identityRecovery.historyPreserved")}</li>
                  <li>{t("identityRecovery.noDetection")}</li>
                  <li>{t("identityRecovery.noRemoteChange")}</li>
                </ul>
              </div>
              <dl className="identity-recovery-meta">
                <div><dt>{t("identityRecovery.createdAt")}</dt><dd>{loading ? t("identityRecovery.loading") : formatDate(status?.created_at ?? null)}</dd></div>
                <div><dt>{t("identityRecovery.lastRotatedAt")}</dt><dd>{loading ? t("identityRecovery.loading") : formatDate(status?.last_rotated_at ?? null)}</dd></div>
              </dl>
              <p className="identity-recovery-confirmation">{t("identityRecovery.confirmation")}</p>
              <button type="button" className="btn-save" onClick={() => void rotate()} disabled={loading || rotating}>
                {rotating ? t("identityRecovery.rotating") : t("identityRecovery.rotate")}
              </button>
            </>
          )}
        </div>
      </ModalSurface>
    </div>
  );
}
