import {
  createContext,
  createElement,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  normalizeCommandError,
  type CommandError,
} from "../utils/commandErrors";

export type ThemePreference = "system" | "dark" | "light";
export type AccentPreset =
  | "sky"
  | "violet"
  | "emerald"
  | "amber"
  | "rose"
  | "white";
export type AppLanguage = "zh" | "en";
export type WebDavAuthMode = "auto" | "basic" | "digest" | "bearer" | "none";
export type LocalSnapshotFrequency = "daily" | "weekly";
export type LocalSnapshotRetention = 7 | 30 | 90;
export type CredentialStatusKind =
  | "configured"
  | "not_configured"
  | "unavailable"
  | "denied"
  | "invalid"
  | "ambiguous"
  | "migration_required"
  | "recovery_required";
export type SettingsRecoveryStatus =
  | "none"
  | "backup_restored"
  | "defaults_loaded";

export interface CredentialStatus {
  kind: CredentialStatusKind;
  action_required: boolean;
}

export interface SettingsView {
  auto_start: boolean;
  minimize_to_tray: boolean;
  theme: ThemePreference;
  accent_preset: AccentPreset;
  language: AppLanguage;
  webdav_url: string;
  webdav_username: string;
  webdav_auth_mode: WebDavAuthMode;
  webdav_timeout_secs: number;
  auto_sync: boolean;
  sync_interval_minutes: number;
  /** Optional only while older frontend-provided boot/test values remain supported. */
  local_snapshot_enabled?: boolean;
  local_snapshot_frequency?: LocalSnapshotFrequency;
  local_snapshot_retention?: LocalSnapshotRetention;
  /** Backend-owned latch set by a full-vault restore until a user sync succeeds. */
  sync_confirmation_required?: boolean;
  editor_line_wrap: boolean;
  editor_code_completion: boolean;
  last_sync_at: string;
  webdav_secret_configured: boolean;
  credential_status: CredentialStatus;
  settings_recovery_status: SettingsRecoveryStatus;
}

export interface SettingsDraft {
  auto_start: boolean;
  minimize_to_tray: boolean;
  theme: ThemePreference;
  accent_preset: AccentPreset;
  language: AppLanguage;
  webdav_url: string;
  webdav_username: string;
  webdav_auth_mode: WebDavAuthMode;
  webdav_timeout_secs: number;
  auto_sync: boolean;
  sync_interval_minutes: number;
  local_snapshot_enabled: boolean;
  local_snapshot_frequency: LocalSnapshotFrequency;
  local_snapshot_retention: LocalSnapshotRetention;
  editor_line_wrap: boolean;
  editor_code_completion: boolean;
}

export type SecretAction =
  | { action: "keep" }
  | { action: "replace"; value: string }
  | { action: "clear" };

export interface SyncResult {
  success: boolean;
  message: string;
  uploaded: boolean;
  uploaded_count: number;
  downloaded_count: number;
  deleted_count: number;
  conflict_count: number;
  pending_count: number;
  protocol_version: number;
  manifest_generation: number;
  total_count: number;
}

export interface SyncVersion {
  id: string;
  synced_at: string;
  direction: string;
  snippet_count: number;
  uploaded_count: number;
  downloaded_count: number;
  deleted_count: number;
  conflict_count: number;
  protocol_version: number;
  generation: number;
  message: string;
}

export type SyncNotificationCategory =
  | "success"
  | "pending"
  | "conflict"
  | "failure"
  | "busy"
  | "restore_required";

export interface SyncNotification {
  id: string;
  occurred_at: string;
  source: SyncSource;
  status: SyncCompletionStatus;
  category: SyncNotificationCategory;
  error_code: CommandError["code"] | null;
  retryable: boolean;
  uploaded_count: number | null;
  downloaded_count: number | null;
  deleted_count: number | null;
  conflict_count: number | null;
  pending_count: number | null;
  total_count: number | null;
  protocol_version: number | null;
  manifest_generation: number | null;
  read_at: string | null;
}

export type SyncSource = "toolbar" | "settings" | "tray" | "background";
export type SyncCompletionStatus = "result" | "error" | "busy";

export interface SyncCompletionEvent {
  source: SyncSource;
  status: SyncCompletionStatus;
  result?: SyncResult | null;
  error?: CommandError | null;
}

export interface SettingsApi {
  load: () => Promise<SettingsView>;
  save: (settings: SettingsDraft, secretAction: SecretAction) => Promise<SettingsView>;
  sync: (source: Extract<SyncSource, "toolbar" | "settings">) => Promise<SyncResult>;
  getSyncVersions: () => Promise<SyncVersion[]>;
  getSyncNotifications?: (limit?: number) => Promise<SyncNotification[]>;
  markSyncNotificationRead?: (id: string) => Promise<void>;
  dismissSyncNotification?: (id: string) => Promise<void>;
  markAllSyncNotificationsRead?: () => Promise<void>;
  getSystemTheme: () => Promise<string>;
  getSystemLocale: () => Promise<string>;
}

const defaultApi: SettingsApi = {
  load: () => invoke<SettingsView>("get_settings"),
  save: (settings, secretAction) =>
    invoke<SettingsView>("save_settings", {
      newSettings: settings,
      secretAction,
    }),
  sync: (source) => invoke<SyncResult>("sync_upload", { source }),
  getSyncVersions: () => invoke<SyncVersion[]>("get_sync_versions"),
  getSyncNotifications: (limit = 50) =>
    invoke<SyncNotification[]>("list_sync_notifications", { limit }),
  markSyncNotificationRead: (id) =>
    invoke<void>("mark_sync_notification_read", { id }),
  dismissSyncNotification: (id) =>
    invoke<void>("dismiss_sync_notification", { id }),
  markAllSyncNotificationsRead: () =>
    invoke<void>("mark_all_sync_notifications_read"),
  getSystemTheme: () => invoke<string>("get_system_theme"),
  getSystemLocale: () => invoke<string>("get_system_locale"),
};

export interface SettingsContextValue {
  settings: SettingsView | null;
  loading: boolean;
  saving: boolean;
  syncing: boolean;
  error: CommandError | null;
  syncHistory: SyncVersion[];
  historyLoading: boolean;
  historyError: CommandError | null;
  syncNotifications: SyncNotification[];
  notificationsLoading: boolean;
  notificationsError: CommandError | null;
  syncStatus: SyncCompletionEvent | null;
  reload: () => Promise<SettingsView>;
  load: () => Promise<SettingsView>;
  save: (draft: SettingsDraft, secretAction?: SecretAction) => Promise<SettingsView>;
  sync: (source: Extract<SyncSource, "toolbar" | "settings">) => Promise<SyncResult>;
  reloadHistory: () => Promise<SyncVersion[]>;
  reloadNotifications: () => Promise<SyncNotification[]>;
  markSyncNotificationRead: (id: string) => Promise<void>;
  dismissSyncNotification: (id: string) => Promise<void>;
  markAllSyncNotificationsRead: () => Promise<void>;
  getSyncVersions: () => Promise<SyncVersion[]>;
  getSystemTheme: () => Promise<string>;
  getSystemLocale: () => Promise<string>;
  setSyncStatus: (status: SyncCompletionEvent | null) => void;
}

export const SettingsContext = createContext<SettingsContextValue | null>(null);

interface SettingsProviderProps {
  children?: ReactNode;
  initialSettings?: SettingsView | null | Promise<SettingsView | null>;
  api?: Partial<SettingsApi>;
}

export function settingsToDraft(settings: SettingsView): SettingsDraft {
  return {
    auto_start: settings.auto_start,
    minimize_to_tray: settings.minimize_to_tray,
    theme: settings.theme,
    accent_preset: settings.accent_preset,
    language: settings.language,
    webdav_url: settings.webdav_url,
    webdav_username: settings.webdav_username,
    webdav_auth_mode: settings.webdav_auth_mode,
    webdav_timeout_secs: settings.webdav_timeout_secs,
    auto_sync: settings.auto_sync,
    sync_interval_minutes: settings.sync_interval_minutes,
    local_snapshot_enabled: settings.local_snapshot_enabled ?? false,
    local_snapshot_frequency: settings.local_snapshot_frequency ?? "daily",
    local_snapshot_retention: settings.local_snapshot_retention ?? 7,
    editor_line_wrap: settings.editor_line_wrap,
    editor_code_completion: settings.editor_code_completion ?? true,
  };
}

export function SettingsProvider({
  children,
  initialSettings,
  api: apiOverrides,
}: SettingsProviderProps) {
  const api = useMemo(
    () => ({ ...defaultApi, ...apiOverrides }),
    [apiOverrides],
  );
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [syncHistory, setSyncHistory] = useState<SyncVersion[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [historyError, setHistoryError] = useState<CommandError | null>(null);
  const [syncNotifications, setSyncNotifications] = useState<SyncNotification[]>([]);
  const [notificationsLoading, setNotificationsLoading] = useState(false);
  const [notificationsError, setNotificationsError] = useState<CommandError | null>(null);
  const [syncStatus, setSyncStatus] = useState<SyncCompletionEvent | null>(null);
  const settingsRef = useRef<SettingsView | null>(null);
  const latestReloadRef = useRef(0);
  const latestSaveRef = useRef(0);
  const activeSyncsRef = useRef(0);
  const latestNotificationsRef = useRef(0);

  const applySettings = useCallback((next: SettingsView) => {
    settingsRef.current = next;
    setSettings(next);
  }, []);

  const reload = useCallback(async () => {
    const requestId = ++latestReloadRef.current;
    setLoading(true);
    setError(null);
    try {
      const next = await api.load();
      if (requestId === latestReloadRef.current) {
        applySettings(next);
      }
      return next;
    } catch (cause) {
      if (requestId === latestReloadRef.current) {
        setError(normalizeCommandError(cause));
      }
      throw cause;
    } finally {
      if (requestId === latestReloadRef.current) {
        setLoading(false);
      }
    }
  }, [api, applySettings]);

  useEffect(() => {
    let cancelled = false;

    if (initialSettings !== undefined) {
      Promise.resolve(initialSettings)
        .then((next) => {
          if (cancelled) return;
          if (next) applySettings(next);
          else setError(normalizeCommandError(null));
        })
        .finally(() => {
          if (!cancelled) setLoading(false);
        });
    } else {
      void reload().catch(() => {});
    }

    return () => {
      cancelled = true;
    };
  }, [applySettings, initialSettings, reload]);

  const save = useCallback(
    async (draft: SettingsDraft, secretAction: SecretAction = { action: "keep" }) => {
      if (!settingsRef.current) {
        const missing = normalizeCommandError(null);
        setError(missing);
        throw missing;
      }

      const requestId = ++latestSaveRef.current;
      latestReloadRef.current += 1;
      setSaving(true);
      setError(null);
      try {
        const saved = await api.save(draft, secretAction);
        if (requestId === latestSaveRef.current) {
          latestReloadRef.current += 1;
          applySettings(saved);
        }
        return saved;
      } catch (cause) {
        if (requestId === latestSaveRef.current) {
          setError(normalizeCommandError(cause));
        }
        throw cause;
      } finally {
        if (requestId === latestSaveRef.current) {
          setSaving(false);
          setLoading(false);
        }
      }
    },
    [api, applySettings],
  );

  const sync = useCallback(
    async (source: Extract<SyncSource, "toolbar" | "settings">) => {
      activeSyncsRef.current += 1;
      setSyncing(true);
      try {
        return await api.sync(source);
      } finally {
        activeSyncsRef.current -= 1;
        if (activeSyncsRef.current === 0) setSyncing(false);
      }
    },
    [api],
  );

  const reloadHistory = useCallback(async () => {
    setHistoryLoading(true);
    setHistoryError(null);
    try {
      const versions = await api.getSyncVersions();
      setSyncHistory(versions);
      return versions;
    } catch (cause) {
      setHistoryError(normalizeCommandError(cause));
      throw cause;
    } finally {
      setHistoryLoading(false);
    }
  }, [api]);

  const reloadNotifications = useCallback(async () => {
    const requestId = ++latestNotificationsRef.current;
    setNotificationsLoading(true);
    setNotificationsError(null);
    try {
      const notifications = await api.getSyncNotifications?.() ?? [];
      if (requestId === latestNotificationsRef.current) {
        setSyncNotifications(notifications);
      }
      return notifications;
    } catch (cause) {
      if (requestId === latestNotificationsRef.current) {
        setNotificationsError(normalizeCommandError(cause));
      }
      throw cause;
    } finally {
      if (requestId === latestNotificationsRef.current) {
        setNotificationsLoading(false);
      }
    }
  }, [api]);

  const markSyncNotificationRead = useCallback(async (id: string) => {
    if (!api.markSyncNotificationRead) return;
    await api.markSyncNotificationRead(id);
    latestNotificationsRef.current += 1;
    setSyncNotifications((notifications) =>
      notifications.map((notification) =>
        notification.id === id && notification.read_at === null
          ? { ...notification, read_at: new Date().toISOString() }
          : notification,
      ),
    );
  }, [api]);

  const dismissSyncNotification = useCallback(async (id: string) => {
    if (!api.dismissSyncNotification) return;
    await api.dismissSyncNotification(id);
    latestNotificationsRef.current += 1;
    setSyncNotifications((notifications) =>
      notifications.filter((notification) => notification.id !== id),
    );
  }, [api]);

  const markAllSyncNotificationsRead = useCallback(async () => {
    if (!api.markAllSyncNotificationsRead) return;
    await api.markAllSyncNotificationsRead();
    latestNotificationsRef.current += 1;
    const readAt = new Date().toISOString();
    setSyncNotifications((notifications) =>
      notifications.map((notification) =>
        notification.read_at === null ? { ...notification, read_at: readAt } : notification,
      ),
    );
  }, [api]);

  const value = useMemo<SettingsContextValue>(
    () => ({
      settings,
      loading,
      saving,
      syncing,
      error,
      syncHistory,
      historyLoading,
      historyError,
      syncNotifications,
      notificationsLoading,
      notificationsError,
      syncStatus,
      reload,
      load: reload,
      save,
      sync,
      reloadHistory,
      reloadNotifications,
      markSyncNotificationRead,
      dismissSyncNotification,
      markAllSyncNotificationsRead,
      getSyncVersions: reloadHistory,
      getSystemTheme: api.getSystemTheme,
      getSystemLocale: api.getSystemLocale,
      setSyncStatus,
    }),
    [
      api,
      error,
      historyError,
      historyLoading,
      loading,
      notificationsError,
      notificationsLoading,
      reload,
      reloadHistory,
      reloadNotifications,
      markSyncNotificationRead,
      dismissSyncNotification,
      markAllSyncNotificationsRead,
      save,
      saving,
      settings,
      sync,
      syncHistory,
      syncNotifications,
      syncing,
      syncStatus,
    ],
  );

  return createElement(SettingsContext.Provider, { value }, children);
}

export function useSettings(): SettingsContextValue {
  const context = useContext(SettingsContext);
  if (!context) {
    throw new Error("useSettings must be used within SettingsProvider");
  }
  return context;
}

export type Settings = SettingsView;
