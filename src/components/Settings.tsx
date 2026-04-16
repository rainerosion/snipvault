import React, { useState, useEffect, useCallback, useRef, useContext } from "react";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-shell";
import { useTranslation } from "react-i18next";
import { useSettings, Settings, SyncVersion } from "../hooks/useSettings";
import { LanguageContext } from "../context/LanguageContext";
import { LANGUAGES } from "../i18n";
import { Dialog, DialogHandle } from "./Dialog";

interface SettingsPanelProps {
  theme: "dark" | "light";
  setTheme: (t: "dark" | "light") => void;
  onClose: () => void;
}

export function SettingsPanel({ theme, setTheme, onClose }: SettingsPanelProps) {
  const { t } = useTranslation();
  const { setLanguage } = useContext(LanguageContext);
  const { settings, loading, load, save, syncUpload, getSyncVersions, getSystemTheme } = useSettings();
  const [form, setForm] = useState<Settings | null>(null);
  const [syncing, setSyncing] = useState(false);

  // Prefer form.language (user's current selection), fall back to saved settings.language
  const currentLang = (form?.language || settings?.language || "zh") as string;
  const [syncMsg, setSyncMsg] = useState<{ type: "success" | "error"; text: string } | null>(null);
  const [saved, setSaved] = useState(false);
  const [syncHistory, setSyncHistory] = useState<SyncVersion[]>([]);
  const [showHistory, setShowHistory] = useState(false);
  const dialogRef = useRef<DialogHandle>(null);

  useEffect(() => { load().catch(() => {}); }, [load]);

  useEffect(() => {
    if (settings) setForm({ ...settings });
  }, [settings]);

  useEffect(() => {
    if (showHistory) {
      getSyncVersions().then(setSyncHistory).catch(() => setSyncHistory([]));
    }
  }, [showHistory, getSyncVersions]);

  const applyTheme = useCallback(async (themeVal: string) => {
    if (themeVal === "dark" || themeVal === "light") {
      setTheme(themeVal);
      window.dispatchEvent(new CustomEvent("snipvault-theme-pref-changed", {
        detail: { pref: themeVal, effective: themeVal },
      }));
    } else if (themeVal === "system") {
      const sys = await getSystemTheme().catch(() => "dark");
      const effective = sys === "light" ? "light" : "dark";
      setTheme(effective);
      window.dispatchEvent(new CustomEvent("snipvault-theme-pref-changed", {
        detail: { pref: "system", effective },
      }));
    }
  }, [setTheme, getSystemTheme]);

  const handleSave = useCallback(async () => {
    if (!form) return;
    try {
      // save_settings on backend already syncs autostart state when auto_start changes
      await save(form);
      await applyTheme(form.theme);
      if (form.language !== settings?.language) {
        setLanguage(form.language);
      }
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      dialogRef.current?.alert(t("errors.settingsFailed", { error: e }));
    }
  }, [form, settings, save, applyTheme, setLanguage, t]);

  const handleSync = useCallback(async () => {
    const ok = await dialogRef.current?.confirm(t("settings.syncConfirm"));
    if (ok !== true) return;
    setSyncing(true);
    setSyncMsg(null);
    try {
      const result = await syncUpload();
      setSyncMsg({ type: result.success ? "success" : "error", text: result.message });
      await load();
    } catch (e) {
      setSyncMsg({ type: "error", text: String(e) });
    } finally {
      setSyncing(false);
    }
  }, [syncUpload, load, t]);

  const formatDate = (iso: string) => {
    if (!iso) return t("settings.neverSynced");
    try {
      return new Date(iso).toLocaleString(currentLang === "zh" ? "zh-CN" : "en-US", { hour12: false });
    } catch { return iso; }
  };

  if (loading) {
    return (
      <div className="settings-panel">
        <div className="settings-loading-inline"><div className="spinner" /></div>
      </div>
    );
  }
  if (!form) {
    return (
      <div className="settings-panel">
        <div className="settings-loading-inline">
          <span style={{ color: "var(--text-muted)", fontSize: 13 }}>{t("errors.loadFailed")}</span>
        </div>
      </div>
    );
  }

  const effectiveTheme = (form.theme === "system" ? theme : form.theme) as "dark" | "light";

  return (
    <div className="settings-panel">
      <Dialog ref={dialogRef} theme={effectiveTheme} />
      <div className="settings-header">
        <h2 className="settings-title">{t("settings.title")}</h2>
        <button className="settings-close" onClick={onClose} title={t("settings.close")}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/>
          </svg>
        </button>
      </div>

      <div className="settings-body">
        <section className="settings-section">
          <h3 className="settings-section-title">{t("settings.general")}</h3>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.autoStart")}</span>
              <span className="settings-row-desc">{t("settings.autoStartDesc")}</span>
            </div>
            <input type="checkbox" className="settings-toggle"
              checked={form.auto_start}
              onChange={(e) => setForm({ ...form, auto_start: e.target.checked })} />
          </label>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.minimizeToTray")}</span>
              <span className="settings-row-desc">{t("settings.minimizeToTrayDesc")}</span>
            </div>
            <input type="checkbox" className="settings-toggle"
              checked={form.minimize_to_tray}
              onChange={(e) => setForm({ ...form, minimize_to_tray: e.target.checked })} />
          </label>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.theme")}</span>
            </div>
            <select className="settings-select"
              value={form.theme}
              onChange={(e) => setForm({ ...form, theme: e.target.value })}>
              <option value="system">{t("settings.themeSystem")}</option>
              <option value="dark">{t("settings.themeDark")}</option>
              <option value="light">{t("settings.themeLight")}</option>
            </select>
          </label>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.language")}</span>
              <span className="settings-row-desc">{t("settings.languageDesc")}</span>
            </div>
            <select className="settings-select"
              value={form.language}
              onChange={(e) => setForm({ ...form, language: e.target.value })}>
              {LANGUAGES.map((l) => (
                <option key={l.code} value={l.code}>{l.nativeName}</option>
              ))}
            </select>
          </label>
        </section>

        <section className="settings-section">
          <h3 className="settings-section-title">{t("settings.webdav")}</h3>
          <p className="settings-section-desc">{t("settings.webdavDesc")}</p>

          <div className="settings-field">
            <label className="settings-field-label">{t("settings.webdavUrl")}</label>
            <input className="settings-input"
              placeholder={t("settings.webdavUrlPlaceholder")}
              value={form.webdav_url}
              onChange={(e) => setForm({ ...form, webdav_url: e.target.value })} />
          </div>
          <div className="settings-field">
            <label className="settings-field-label">{t("settings.username")}</label>
            <input className="settings-input" placeholder={t("settings.usernamePlaceholder")}
              value={form.webdav_username}
              onChange={(e) => setForm({ ...form, webdav_username: e.target.value })} />
          </div>
          <div className="settings-field">
            <label className="settings-field-label">{t("settings.password")}</label>
            <input className="settings-input" type="password" placeholder={t("settings.passwordPlaceholder")}
              value={form.webdav_password}
              onChange={(e) => setForm({ ...form, webdav_password: e.target.value })} />
          </div>

          <label className="settings-row">
            <div className="settings-row-info">
              <span className="settings-row-label">{t("settings.timeout")}</span>
            </div>
            <select className="settings-select"
              value={form.webdav_timeout_secs}
              onChange={(e) => setForm({ ...form, webdav_timeout_secs: Number(e.target.value) })}>
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
            <input type="checkbox" className="settings-toggle"
              checked={form.auto_sync}
              onChange={(e) => setForm({ ...form, auto_sync: e.target.checked })} />
          </label>

          {form.auto_sync && (
            <label className="settings-row">
              <div className="settings-row-info">
                <span className="settings-row-label">{t("settings.syncInterval")}</span>
              </div>
              <select className="settings-select"
                value={form.sync_interval_minutes}
                onChange={(e) => setForm({ ...form, sync_interval_minutes: Number(e.target.value) })}>
                <option value={5}>{currentLang === "zh" ? "5 分钟" : "5 min"}</option>
                <option value={15}>{currentLang === "zh" ? "15 分钟" : "15 min"}</option>
                <option value={30}>{currentLang === "zh" ? "30 分钟" : "30 min"}</option>
                <option value={60}>{currentLang === "zh" ? "1 小时" : "1 hr"}</option>
                <option value={120}>{currentLang === "zh" ? "2 小时" : "2 hr"}</option>
              </select>
            </label>
          )}

          <div className="settings-sync-actions">
            <button className="btn-sync-upload" onClick={handleSync} disabled={syncing || !form.webdav_url}>
              {syncing ? t("settings.syncInProgress") : t("settings.syncNow")}
            </button>
            <button className="btn-sync-history" onClick={() => setShowHistory(!showHistory)}>
              {showHistory ? t("settings.collapseHistory") : t("settings.syncHistory")}
            </button>
          </div>

          {syncMsg && (
            <div className={`sync-msg ${syncMsg.type}`}>{syncMsg.text}</div>
          )}

          {form.last_sync_at && (
            <div className="sync-last-time">{t("settings.lastSync", { time: formatDate(form.last_sync_at) })}</div>
          )}

          {showHistory && (
            <div className="sync-history-list">
              {syncHistory.length === 0 ? (
                <div className="sync-history-empty">{t("settings.noHistory")}</div>
              ) : (
                syncHistory.map((v) => (
                  <div key={v.id} className="sync-history-item">
                    <span className="sync-history-time">{formatDate(v.synced_at)}</span>
                    <span className="sync-history-dir">{v.direction}</span>
                    <span className="sync-history-count">{v.snippet_count} {currentLang === "zh" ? "条" : "items"}</span>
                    <span className="sync-history-msg">{v.message}</span>
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
              <span className="about-value">灵藏 · SnipVault</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutVersion")}</span>
              <span className="about-value">v1.0.8</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutAuthor")}</span>
              <span className="about-value">浅语</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutDesc")}</span>
              <span className="about-value">{t("settings.aboutValueDesc")}</span>
            </div>
            <div className="about-row">
              <span className="about-label">{t("settings.aboutRepo")}</span>
              <span
                className="about-link"
                style={{ cursor: "pointer" }}
                onClick={() => open("https://github.com/rainerosion/snipvault")}
              >
                https://github.com/rainerosion/snipvault
              </span>
            </div>
          </div>
        </section>
      </div>

      <div className="settings-footer">
        <span className="settings-version">灵藏 · SnipVault v1.0.8</span>
        <div className="settings-footer-btns">
          <button className="btn-save-settings" onClick={handleSave}>
            {saved ? t("settings.saved") : t("settings.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
