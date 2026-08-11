import type { Settings } from "../hooks/useSettings";

export const DEFAULT_SETTINGS: Settings = {
  auto_start: false,
  minimize_to_tray: true,
  theme: "system",
  accent_preset: "sky",
  language: "en",
  webdav_url: "https://example.test/dav",
  webdav_username: "user",
  webdav_auth_mode: "auto",
  webdav_timeout_secs: 30,
  auto_sync: false,
  sync_interval_minutes: 30,
  editor_line_wrap: true,
  last_sync_at: "2026-01-01T00:00:00Z",
  webdav_secret_configured: true,
  credential_status: { kind: "configured", action_required: false },
  settings_recovery_status: "none",
};
