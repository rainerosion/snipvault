import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";

export interface Settings {
  auto_start: boolean;
  minimize_to_tray: boolean;
  theme: string;
  language: string;
  webdav_url: string;
  webdav_username: string;
  webdav_password: string;
  webdav_timeout_secs: number;
  auto_sync: boolean;
  sync_interval_minutes: number;
  last_sync_at: string;
}

export interface SyncResult {
  success: boolean;
  message: string;
  uploaded: boolean;
  uploaded_count: number;
  downloaded_count: number;
  total_count: number;
}

export interface SyncVersion {
  id: string;
  synced_at: string;
  direction: string;
  snippet_count: number;
  uploaded_count: number;
  downloaded_count: number;
  message: string;
}

export function useSettings() {
  const [settings, setSettings] = useState<Settings | null>(null);
  const [loading, setLoading] = useState(true);

  const load = useCallback(async () => {
    setLoading(true);
    try {
      const data = await invoke<Settings>("get_settings");
      setSettings(data);
    } catch (e) {
      console.error("[useSettings] load failed:", e);
      throw e;
    } finally {
      setLoading(false);
    }
  }, []);

  const save = useCallback(async (s: Settings) => {
    await invoke("save_settings", { newSettings: s });
    setSettings(s);
  }, []);

  const setAutoStart = useCallback(async (enabled: boolean) => {
    await invoke("set_auto_start", { enabled });
  }, []);

  const syncUpload = useCallback(async () => {
    return invoke<SyncResult>("sync_upload");
  }, []);

  const syncDownload = useCallback(async () => {
    return invoke<SyncResult>("sync_download");
  }, []);

  const getSyncVersions = useCallback(async () => {
    return invoke<SyncVersion[]>("get_sync_versions");
  }, []);

  const getSystemTheme = useCallback(async () => {
    return invoke<string>("get_system_theme");
  }, []);

  const getSystemLocale = useCallback(async () => {
    return invoke<string>("get_system_locale");
  }, []);

  return { settings, loading, load, save, setAutoStart, syncUpload, syncDownload, getSyncVersions, getSystemTheme, getSystemLocale };
}
