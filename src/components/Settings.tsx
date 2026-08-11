import {
  forwardRef,
  useCallback,
  useContext,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import { useTranslation } from "react-i18next";
import {
  settingsToDraft,
  useSettings,
  type SecretAction,
  type SettingsDraft,
  type SyncCompletionEvent,
  type SyncResult,
} from "../hooks/useSettings";
import { LanguageContext } from "../context/LanguageContext";
import { LANGUAGES } from "../i18n";
import { localizeCommandError } from "../utils/commandErrors";
import { ACCENT_PRESETS } from "../theme";
import { Dialog, type DialogHandle } from "./Dialog";
import { ModalSurface } from "./ModalSurface";

const APP_NAME = "灵藏 · SnipVault";
const APP_VERSION = import.meta.env.VITE_APP_VERSION;
interface SettingsPanelProps {
  /** @deprecated Appearance is derived from the root ThemeProvider. */
  theme?: "dark" | "light";
  /** @deprecated Appearance is derived from the root ThemeProvider. */
  setTheme?: (theme: "dark" | "light") => void;
  onClose: () => void;
  onSync: () => Promise<SyncCompletionEvent>;
}

export interface SettingsPanelHandle {
  requestClose: () => Promise<boolean>;
}

const WEBDAV_DRAFT_KEYS: Array<keyof SettingsDraft> = [
  "webdav_url",
  "webdav_username",
  "webdav_auth_mode",
  "webdav_timeout_secs",
  "auto_sync",
  "sync_interval_minutes",
];

function areSettingsDraftsEqual(
  left: SettingsDraft,
  right: SettingsDraft,
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function isWebDavDraftDirty(
  draft: SettingsDraft,
  baseline: SettingsDraft,
): boolean {
  return WEBDAV_DRAFT_KEYS.some((key) => draft[key] !== baseline[key]);
}

export const SettingsPanel = forwardRef<
  SettingsPanelHandle,
  SettingsPanelProps
>(function SettingsPanel({ onClose, onSync }, ref) {
  const { t } = useTranslation();
  const { setLanguage } = useContext(LanguageContext);
  const {
    settings,
    loading,
    saving,
    syncing,
    error,
    syncHistory,
    historyLoading,
    historyError,
    reload,
    save,
    reloadHistory,
  } = useSettings();
  const [draft, setDraft] = useState<SettingsDraft | null>(null);
  const [baseline, setBaseline] = useState<SettingsDraft | null>(null);
  const [externalChange, setExternalChange] = useState(false);
  const [syncMsg, setSyncMsg] = useState<{
    type: "success" | "error";
    text: string;
    result?: SyncResult;
  } | null>(null);
  const [saved, setSaved] = useState(false);
  const [secretInput, setSecretInput] = useState("");
  const [secretAction, setSecretAction] = useState<SecretAction>({ action: "keep" });
  const [showHistory, setShowHistory] = useState(false);
  const dialogRef = useRef<DialogHandle>(null);
  const draftRef = useRef<SettingsDraft | null>(null);
  const baselineRef = useRef<SettingsDraft | null>(null);
  const requestCloseRef = useRef<() => Promise<boolean>>(async () => false);
  const closePendingRef = useRef(false);

  const setDraftState = useCallback((next: SettingsDraft) => {
    draftRef.current = next;
    setDraft(next);
  }, []);

  const setBaselineState = useCallback((next: SettingsDraft) => {
    baselineRef.current = next;
    setBaseline(next);
  }, []);

  useEffect(() => {
    if (!settings) return;
    const authoritative = settingsToDraft(settings);
    const currentDraft = draftRef.current;
    const currentBaseline = baselineRef.current;

    if (!currentDraft || !currentBaseline) {
      setDraftState(authoritative);
      setBaselineState(authoritative);
      setExternalChange(false);
      return;
    }

    const dirty = !areSettingsDraftsEqual(currentDraft, currentBaseline);
    if (!dirty) {
      setDraftState(authoritative);
      setBaselineState(authoritative);
      setExternalChange(false);
    } else if (!areSettingsDraftsEqual(authoritative, currentBaseline)) {
      setExternalChange(true);
    }
  }, [settings, setBaselineState, setDraftState]);

  useEffect(() => {
    if (showHistory) {
      void reloadHistory().catch(() => {});
    }
  }, [showHistory, reloadHistory]);

  const currentLang = draft?.language || settings?.language || "zh";
  const syncHistoryDirectionLabels = useMemo(
    () =>
      ({
        publish: t("settings.syncHistoryDirectionPublish"),
        merge: t("settings.syncHistoryDirectionMerge"),
      }) as Record<string, string>,
    [t],
  );
  const formatHistoryDirection = useCallback(
    (direction: string) =>
      syncHistoryDirectionLabels[direction] ||
      t("settings.syncHistoryDirectionOther", { direction }),
    [syncHistoryDirectionLabels, t],
  );
  const formatHistoryCounts = useCallback(
    (version: {
      snippet_count: number;
      uploaded_count: number;
      downloaded_count: number;
      deleted_count: number;
      conflict_count: number;
    }) =>
      t("settings.syncHistoryCounts", {
        total: version.snippet_count,
        uploaded: version.uploaded_count,
        downloaded: version.downloaded_count,
        deleted: version.deleted_count,
        conflicts: version.conflict_count,
      }),
    [t],
  );
  const dirty = useMemo(
    () =>
      !!draft &&
      !!baseline &&
      (!areSettingsDraftsEqual(draft, baseline) || secretAction.action !== "keep"),
    [baseline, draft, secretAction.action],
  );
  const webdavDirty = useMemo(
    () =>
      !!draft &&
      !!baseline &&
      (isWebDavDraftDirty(draft, baseline) || secretAction.action !== "keep"),
    [baseline, draft, secretAction.action],
  );

  const updateDraft = useCallback(
    <Key extends keyof SettingsDraft>(key: Key, value: SettingsDraft[Key]) => {
      const current = draftRef.current;
      if (!current) return;
      setSaved(false);
      setDraftState({ ...current, [key]: value });
    },
    [setDraftState],
  );

  const handleSave = useCallback(async (): Promise<boolean> => {
    const submitted = draftRef.current;
    if (!submitted || saving) return false;
    try {
      const savedSettings = await save(submitted, secretAction);
      const savedDraft = settingsToDraft(savedSettings);
      setSecretInput("");
      setSecretAction({ action: "keep" });
      setDraftState(savedDraft);
      setBaselineState(savedDraft);
      setExternalChange(false);
      if (savedDraft.language !== settings?.language) {
        setLanguage(savedDraft.language);
      }
      setSaved(true);
      window.setTimeout(() => setSaved(false), 2000);
      return true;
    } catch (cause) {
      await dialogRef.current?.alert(
        t("errors.settingsFailed", {
          error: localizeCommandError(cause, t),
        }),
      );
      return false;
    }
  }, [
    save,
    saving,
    secretAction,
    setBaselineState,
    setDraftState,
    setLanguage,
    settings?.language,
    t,
  ]);

  const requestClose = useCallback(async (): Promise<boolean> => {
    if (closePendingRef.current) return false;
    const currentDraft = draftRef.current;
    const currentBaseline = baselineRef.current;
    if (!currentDraft || !currentBaseline) {
      onClose();
      return true;
    }

    if (
      areSettingsDraftsEqual(currentDraft, currentBaseline) &&
      secretAction.action === "keep"
    ) {
      onClose();
      return true;
    }

    closePendingRef.current = true;
    try {
      const action = await dialogRef.current?.ask(
        t("settings.unsavedChanges"),
      );
      if (action === "save") {
        const didSave = await handleSave();
        if (!didSave) return false;
      } else if (action !== "discard") {
        return false;
      }
      onClose();
      return true;
    } finally {
      closePendingRef.current = false;
    }
  }, [handleSave, onClose, secretAction.action, t]);

  requestCloseRef.current = requestClose;

  useImperativeHandle(
    ref,
    () => ({ requestClose: () => requestCloseRef.current() }),
    [],
  );

  const handleSync = useCallback(async () => {
    if (webdavDirty) return;
    const confirmed = await dialogRef.current?.confirm(
      t("settings.syncConfirm"),
    );
    if (confirmed !== true) return;

    setSyncMsg(null);
    const completion = await onSync();
    if (completion.status === "result" && completion.result?.success) {
      setSyncMsg({
        type: "success",
        text: completion.result.message,
        result: completion.result,
      });
    } else {
      setSyncMsg({
        type: "error",
        text: completion.error
          ? localizeCommandError(completion.error, t)
          : completion.result?.message || t("errors.syncFailedShort"),
      });
    }
  }, [onSync, t, webdavDirty]);

  const formatDate = (iso: string) => {
    if (!iso) return t("settings.neverSynced");
    try {
      return new Date(iso).toLocaleString(
        currentLang === "zh" ? "zh-CN" : "en-US",
        { hour12: false },
      );
    } catch {
      return iso;
    }
  };

  if (loading && !draft) {
    return (
      <ModalSurface
        className="settings-panel"
        labelledBy="settings-loading-title"
        onEscape={() => void requestCloseRef.current()}
      >
        <h2 id="settings-loading-title" className="sr-only">
          {t("settings.title")}
        </h2>
        <div className="settings-loading-inline" role="status" aria-busy="true">
          <div className="spinner" aria-hidden="true" />
          <span className="sr-only">{t("sidebar.loading")}</span>
        </div>
      </ModalSurface>
    );
  }

  if (!draft) {
    return (
      <ModalSurface
        className="settings-panel"
        labelledBy="settings-error-title"
        onEscape={() => void requestCloseRef.current()}
      >
        <h2 id="settings-error-title" className="sr-only">
          {t("settings.title")}
        </h2>
        <div className="settings-loading-inline" role="alert">
          <span style={{ color: "var(--text-muted)", fontSize: 13 }}>
            {error ? localizeCommandError(error, t) : t("errors.loadFailed")}
          </span>
          <button
            type="button"
            className="snippet-retry-btn"
            onClick={() => void reload().catch(() => {})}
          >
            {t("sidebar.retry")}
          </button>
        </div>
      </ModalSurface>
    );
  }

  const syncDisabled =
    syncing ||
    !draft.webdav_url.trim() ||
    webdavDirty ||
    settings?.credential_status.action_required === true;

  return (
    <ModalSurface
      className="settings-panel"
      labelledBy="settings-title"
      onEscape={() => void requestCloseRef.current()}
    >
      <Dialog ref={dialogRef} />
      <div className="settings-header">
        <h2 id="settings-title" className="settings-title">
          {t("settings.title")}
        </h2>
        <button
          type="button"
          className="settings-close"
          onClick={() => void requestClose()}
          title={t("settings.close")}
          aria-label={t("settings.close")}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <line x1="18" y1="6" x2="6" y2="18" />
            <line x1="6" y1="6" x2="18" y2="18" />
          </svg>
        </button>
      </div>

      {externalChange && (
        <div className="settings-external-status" role="status" aria-live="polite">
          {t("settings.changedExternally")}
        </div>
      )}

      <div className="settings-body">
        {settings && settings.settings_recovery_status !== "none" && (
          <div className="settings-external-status" role="status">
            <span>{t(`settings.recovery.${settings.settings_recovery_status}`)}</span>
            <button
              type="button"
              className="about-link"
              onClick={() =>
                void invoke("open_trusted_directory", { directory: "data" }).catch(
                  async (cause) => {
                    await dialogRef.current?.alert(localizeCommandError(cause, t));
                  },
                )
              }
            >
              {t("settings.openDataFolder")}
            </button>
          </div>
        )}
        <section className="settings-section">
          <h3 className="settings-section-title">{t("settings.general")}</h3>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.autoStart")}</span>
              <span className="settings-row-desc">{t("settings.autoStartDesc")}</span>
            </div>
            <input
              type="checkbox"
              className="settings-toggle"
              checked={draft.auto_start}
              onChange={(event) => updateDraft("auto_start", event.target.checked)}
            />
          </label>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.minimizeToTray")}</span>
              <span className="settings-row-desc">{t("settings.minimizeToTrayDesc")}</span>
            </div>
            <input
              type="checkbox"
              className="settings-toggle"
              checked={draft.minimize_to_tray}
              onChange={(event) => updateDraft("minimize_to_tray", event.target.checked)}
            />
          </label>

          <label className="settings-row">
            <span className="settings-row-label">{t("settings.theme")}</span>
            <select
              className="settings-select"
              value={draft.theme}
              onChange={(event) =>
                updateDraft("theme", event.target.value as SettingsDraft["theme"])
              }
            >
              <option value="system">{t("settings.themeSystem")}</option>
              <option value="dark">{t("settings.themeDark")}</option>
              <option value="light">{t("settings.themeLight")}</option>
            </select>
          </label>

          <div className="settings-accent-group">
            <div className="settings-row-info">
              <span id="accent-preset-label" className="settings-row-label">
                {t("settings.accentPreset")}
              </span>
              <span id="accent-preset-desc" className="settings-row-desc">
                {t("settings.accentPresetDesc")}
              </span>
            </div>
            <div
              className="settings-accent-options"
              role="radiogroup"
              aria-labelledby="accent-preset-label"
              aria-describedby="accent-preset-desc"
            >
              {ACCENT_PRESETS.map((preset) => {
                const selected = draft.accent_preset === preset;
                return (
                  <button
                    key={preset}
                    type="button"
                    className={`settings-accent-option${selected ? " selected" : ""}`}
                    role="radio"
                    aria-checked={selected}
                    tabIndex={selected ? 0 : -1}
                    aria-label={t("settings.accentPresetOption", {
                      color: t(`settings.accent${preset[0].toUpperCase()}${preset.slice(1)}`),
                      selected: selected ? t("settings.accentPresetSelected") : "",
                    })}
                    onClick={() => updateDraft("accent_preset", preset)}
                    onKeyDown={(event) => {
                      const currentIndex = ACCENT_PRESETS.indexOf(preset);
                      let nextIndex: number | null = null;
                      if (event.key === "ArrowRight" || event.key === "ArrowDown") {
                        nextIndex = (currentIndex + 1) % ACCENT_PRESETS.length;
                      } else if (event.key === "ArrowLeft" || event.key === "ArrowUp") {
                        nextIndex = (currentIndex - 1 + ACCENT_PRESETS.length) % ACCENT_PRESETS.length;
                      } else if (event.key === "Home") {
                        nextIndex = 0;
                      } else if (event.key === "End") {
                        nextIndex = ACCENT_PRESETS.length - 1;
                      }

                      if (nextIndex === null) return;
                      event.preventDefault();
                      updateDraft("accent_preset", ACCENT_PRESETS[nextIndex]);
                      const radios = event.currentTarget.parentElement?.querySelectorAll<HTMLButtonElement>(
                        '[role="radio"]',
                      );
                      radios?.[nextIndex]?.focus();
                    }}
                  >
                    <span className={`settings-accent-swatch ${preset}`} aria-hidden="true" />
                    <span className="settings-accent-name">{t(`settings.accent${preset[0].toUpperCase()}${preset.slice(1)}`)}</span>
                    {selected && <span className="settings-accent-check" aria-hidden="true">✓</span>}
                  </button>
                );
              })}
            </div>
          </div>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.language")}</span>
              <span className="settings-row-desc">{t("settings.languageDesc")}</span>
            </div>
            <select
              className="settings-select"
              value={draft.language}
              onChange={(event) =>
                updateDraft("language", event.target.value as SettingsDraft["language"])
              }
            >
              {LANGUAGES.map((language) => (
                <option key={language.code} value={language.code}>
                  {language.nativeName}
                </option>
              ))}
            </select>
          </label>
        </section>

        <section className="settings-section">
          <h3 className="settings-section-title">{t("settings.webdav")}</h3>
          <p className="settings-section-desc">{t("settings.webdavDesc")}</p>

          <div className="settings-field">
            <label className="settings-field-label" htmlFor="webdav-url">
              {t("settings.webdavUrl")}
            </label>
            <input
              id="webdav-url"
              className="settings-input"
              placeholder={t("settings.webdavUrlPlaceholder")}
              value={draft.webdav_url}
              onChange={(event) => updateDraft("webdav_url", event.target.value)}
            />
          </div>
          <div className="settings-field">
            <label className="settings-field-label" htmlFor="webdav-username">
              {t("settings.username")}
            </label>
            <input
              id="webdav-username"
              className="settings-input"
              placeholder={t("settings.usernamePlaceholder")}
              value={draft.webdav_username}
              onChange={(event) => updateDraft("webdav_username", event.target.value)}
            />
          </div>
          <div className="settings-field">
            <label className="settings-field-label" htmlFor="webdav-password">
              {t("settings.password")}
            </label>
            <input
              id="webdav-password"
              className="settings-input"
              type="password"
              autoComplete="new-password"
              placeholder={
                settings?.webdav_secret_configured
                  ? t("settings.passwordStoredPlaceholder")
                  : t("settings.passwordPlaceholder")
              }
              value={secretInput}
              onChange={(event) => {
                const value = event.target.value;
                setSaved(false);
                setSecretInput(value);
                setSecretAction(
                  value ? { action: "replace", value } : { action: "keep" },
                );
              }}
            />
            <div className="settings-credential-actions">
              <span className="settings-row-desc" aria-live="polite">
                {t(`settings.credentialStatus.${settings?.credential_status.kind ?? "not_configured"}`)}
              </span>
              <button
                type="button"
                className="about-link"
                onClick={() => {
                  setSaved(false);
                  setSecretInput("");
                  setSecretAction({ action: "clear" });
                }}
              >
                {t("settings.clearStoredCredential")}
              </button>
              {secretAction.action !== "keep" && (
                <button
                  type="button"
                  className="about-link"
                  onClick={() => {
                    setSecretInput("");
                    setSecretAction({ action: "keep" });
                  }}
                >
                  {t("settings.keepStoredCredential")}
                </button>
              )}
            </div>
            {secretAction.action !== "keep" && (
              <p className="settings-sync-explanation" role="status">
                {t(`settings.secretAction.${secretAction.action}`)}
              </p>
            )}
          </div>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.webdavAuthMode")}</span>
              <span className="settings-row-desc">{t("settings.webdavAuthModeDesc")}</span>
            </div>
            <select
              className="settings-select"
              value={draft.webdav_auth_mode}
              onChange={(event) =>
                updateDraft(
                  "webdav_auth_mode",
                  event.target.value as SettingsDraft["webdav_auth_mode"],
                )
              }
            >
              <option value="basic">{t("settings.webdavAuthBasic")}</option>
              <option value="digest">{t("settings.webdavAuthDigest")}</option>
              <option value="bearer">{t("settings.webdavAuthBearer")}</option>
              <option value="none">{t("settings.webdavAuthNone")}</option>
              <option value="auto">{t("settings.webdavAuthAuto")}</option>
            </select>
          </label>

          <label className="settings-row">
            <span className="settings-row-label">{t("settings.timeout")}</span>
            <select
              className="settings-select"
              value={draft.webdav_timeout_secs}
              onChange={(event) =>
                updateDraft("webdav_timeout_secs", Number(event.target.value))
              }
            >
              <option value={10}>10 {currentLang === "zh" ? "秒" : "s"}</option>
              <option value={30}>30 {currentLang === "zh" ? "秒" : "s"}</option>
              <option value={60}>1 {currentLang === "zh" ? "分钟" : "min"}</option>
              <option value={120}>2 {currentLang === "zh" ? "分钟" : "min"}</option>
            </select>
          </label>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.autoSync")}</span>
              <span className="settings-row-desc">{t("settings.autoSyncDesc")}</span>
            </div>
            <input
              type="checkbox"
              className="settings-toggle"
              checked={draft.auto_sync}
              onChange={(event) => updateDraft("auto_sync", event.target.checked)}
            />
          </label>

          {draft.auto_sync && (
            <>
              <label className="settings-row">
                <span className="settings-row-label">{t("settings.syncInterval")}</span>
                <select
                  className="settings-select"
                  value={draft.sync_interval_minutes}
                  onChange={(event) =>
                    updateDraft("sync_interval_minutes", Number(event.target.value))
                  }
                >
                  <option value={5}>{currentLang === "zh" ? "5 分钟" : "5 min"}</option>
                  <option value={15}>{currentLang === "zh" ? "15 分钟" : "15 min"}</option>
                  <option value={30}>{currentLang === "zh" ? "30 分钟" : "30 min"}</option>
                  <option value={60}>{currentLang === "zh" ? "1 小时" : "1 hr"}</option>
                  <option value={120}>{currentLang === "zh" ? "2 小时" : "2 hr"}</option>
                </select>
              </label>
              <p className="settings-auto-sync-note">
                {t("settings.autoSyncTimingNote")}
              </p>
            </>
          )}

          <div className="settings-sync-actions">
            <button
              type="button"
              className="btn-sync-upload"
              onClick={() => void handleSync()}
              disabled={syncDisabled}
              aria-describedby={webdavDirty ? "sync-save-first" : undefined}
            >
              {syncing ? t("settings.syncInProgress") : t("settings.syncNow")}
            </button>
            <button
              type="button"
              className="btn-sync-history"
              onClick={() => setShowHistory((visible) => !visible)}
              aria-expanded={showHistory}
            >
              {showHistory
                ? t("settings.collapseHistory")
                : t("settings.syncHistory")}
            </button>
          </div>

          {webdavDirty && (
            <p id="sync-save-first" className="settings-sync-explanation" role="status">
              {t("settings.saveBeforeSync")}
            </p>
          )}

          {syncMsg && (
            <div
              className={`sync-msg ${syncMsg.type}`}
              role={syncMsg.type === "error" ? "alert" : "status"}
              aria-live="polite"
            >
              {syncMsg.text}
              {syncMsg.result && (
                <div className="sync-result-details">
                  <span>
                    {t("settings.syncResultProtocol", {
                      protocol: syncMsg.result.protocol_version,
                      generation: syncMsg.result.manifest_generation,
                    })}
                  </span>
                  <span>
                    {t("settings.syncResultCounts", {
                      total: syncMsg.result.total_count,
                      uploaded: syncMsg.result.uploaded_count,
                      downloaded: syncMsg.result.downloaded_count,
                      deleted: syncMsg.result.deleted_count,
                      conflicts: syncMsg.result.conflict_count,
                      pending: syncMsg.result.pending_count,
                    })}
                  </span>
                </div>
              )}
            </div>
          )}

          {settings?.last_sync_at && (
            <div className="sync-last-time">
              {t("settings.lastSync", {
                time: formatDate(settings.last_sync_at),
              })}
            </div>
          )}

          {showHistory && (
            <div className="sync-history-list" aria-busy={historyLoading}>
              {historyLoading ? (
                <div className="sync-history-empty" role="status">
                  {t("settings.syncHistoryLoading")}
                </div>
              ) : historyError ? (
                <div className="sync-history-empty" role="alert">
                  {localizeCommandError(historyError, t)}
                </div>
              ) : syncHistory.length === 0 ? (
                <div className="sync-history-empty">{t("settings.noHistory")}</div>
              ) : (
                syncHistory.map((version) => (
                  <div key={version.id} className="sync-history-item">
                    <span className="sync-history-time">
                      {formatDate(version.synced_at)}
                    </span>
                    <span className="sync-history-dir">
                      {formatHistoryDirection(version.direction)}
                    </span>
                    <span className="sync-history-count">
                      {formatHistoryCounts(version)}
                    </span>
                    <span className="sync-history-protocol">
                      {t("settings.syncHistoryProtocol", {
                        protocol: version.protocol_version,
                        generation: version.generation,
                      })}
                    </span>
                    {version.message && (
                      <span className="sync-history-msg">{version.message}</span>
                    )}
                  </div>
                ))
              )}
            </div>
          )}
        </section>

        <section className="settings-section">
          <h3 className="settings-section-title">{t("settings.about")}</h3>
          <div className="about-info">
            <div className="about-row">
              <span className="about-label">{t("settings.aboutName")}</span>
              <span className="about-value">{APP_NAME}</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutVersion")}</span>
              <span className="about-value">v{APP_VERSION}</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutAuthor")}</span>
              <span className="about-value">浅语 & AI</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutDesc")}</span>
              <span className="about-value">{t("settings.aboutValueDesc")}</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutRepo")}</span>
              <button
                type="button"
                className="about-link"
                onClick={() =>
                  void invoke("open_project_repository").catch(async (cause) => {
                    await dialogRef.current?.alert(localizeCommandError(cause, t));
                  })
                }
              >
                https://github.com/rainerosion/snipvault
              </button>
            </div>
          </div>
        </section>
      </div>

      <div className="settings-footer">
        <span className="settings-version">
          {APP_NAME} v{APP_VERSION}
        </span>
        <div className="settings-footer-btns">
          <button
            type="button"
            className="btn-save-settings"
            onClick={() => void handleSave()}
            disabled={!dirty || saving}
          >
            {saving
              ? t("settings.saving")
              : saved
                ? t("settings.saved")
                : t("settings.save")}
          </button>
        </div>
      </div>
    </ModalSurface>
  );
});
