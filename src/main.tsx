import React, {
  createContext,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./index.css";
import { LanguageProvider } from "./context/LanguageProvider";
import type { AccentPreset, SettingsView } from "./hooks/useSettings";
import { SettingsProvider, settingsToDraft, useSettings } from "./hooks/useSettings";
import {
  ACCENT_PRESET_KEY,
  THEME_EFFECTIVE_KEY,
  THEME_PREF_KEY,
  applyAppearance,
  getSystemTheme,
  normalizeAccentPreset,
  normalizeThemePreference,
  resolveBootTheme,
  type EffectiveTheme,
} from "./theme";

declare global {
  interface Window {
    __bootT0?: number;
    __bootMark?: (stage: string) => void;
    __bootMarkToNative?: (stage: string) => void;
  }
}

function getCachedAccentPreset(): AccentPreset {
  return normalizeAccentPreset(localStorage.getItem(ACCENT_PRESET_KEY));
}

window.__bootMarkToNative = (stage: string) => {
  const tMs = performance.now() - (window.__bootT0 ?? 0);
  window.__bootMark?.(stage);
  void invoke("boot_mark", { stage, tMs }).catch(() => {});
};

window.__bootMarkToNative("main_eval_start");

const root = document.getElementById("root")!;
const bootTheme = resolveBootTheme();
const bootAccentPreset = getCachedAccentPreset();
applyAppearance(bootTheme, bootAccentPreset);

let bootSettingsPromise: Promise<SettingsView | null> | null = null;

function getBootSettings() {
  if (!bootSettingsPromise) {
    const t = performance.now() - (window.__bootT0 ?? 0);
    window.__bootMark?.("get_settings_start");
    void invoke("boot_mark", { stage: "get_settings_start", tMs: t }).catch(
      () => {},
    );

    bootSettingsPromise = invoke<SettingsView>("get_settings")
      .then((settings) => {
        const done = performance.now() - (window.__bootT0 ?? 0);
        window.__bootMark?.("get_settings_done");
        void invoke("boot_mark", {
          stage: "get_settings_done",
          tMs: done,
        }).catch(() => {});
        return settings;
      })
      .catch(() => {
        const fail = performance.now() - (window.__bootT0 ?? 0);
        window.__bootMark?.("get_settings_fail");
        void invoke("boot_mark", {
          stage: "get_settings_fail",
          tMs: fail,
        }).catch(() => {});
        return null;
      });
  }
  return bootSettingsPromise;
}

function getBootLanguage(): Promise<string> {
  return getBootSettings().then(async (settings) => {
    const initialLanguage = settings?.language || "zh";
    if (settings?.language || !settings) return initialLanguage;

    try {
      const systemLanguage = (await invoke<string>("get_system_locale")) || "zh";
      if (systemLanguage !== initialLanguage) {
        void invoke("save_settings", {
          newSettings: { ...settingsToDraft(settings), language: systemLanguage },
          secretAction: { action: "keep" },
        }).catch(() => {});
      }
      return systemLanguage;
    } catch {
      return initialLanguage;
    }
  });
}

export const ThemeContext = createContext<{
  theme: EffectiveTheme;
  accentPreset: AccentPreset;
  setTheme: (theme: EffectiveTheme) => void;
}>({ theme: bootTheme, accentPreset: bootAccentPreset, setTheme: () => {} });

window.__bootMark?.("react_render_start");
void invoke("boot_mark", {
  stage: "react_render_start",
  tMs: performance.now() - (window.__bootT0 ?? 0),
}).catch(() => {});

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <SettingsProvider initialSettings={getBootSettings()}>
      <ThemeProvider>
        <LanguageProvider loadInitialLanguage={getBootLanguage}>
          <App />
        </LanguageProvider>
      </ThemeProvider>
    </SettingsProvider>
  </React.StrictMode>,
);

window.__bootMark?.("react_render_called");
void invoke("boot_mark", {
  stage: "react_render_called",
  tMs: performance.now() - (window.__bootT0 ?? 0),
}).catch(() => {});

void invoke("frontend_ready", { phase: "react_render_called" }).catch(() => {});
window.__bootMark?.("frontend_ready_sent");
void invoke("boot_mark", {
  stage: "frontend_ready_sent",
  tMs: performance.now() - (window.__bootT0 ?? 0),
}).catch(() => {});

requestAnimationFrame(() => {
  window.__bootMark?.("raf_1");
  void invoke("boot_mark", {
    stage: "raf_1",
    tMs: performance.now() - (window.__bootT0 ?? 0),
  }).catch(() => {});

  requestAnimationFrame(() => {
    window.__bootMark?.("raf_2");
    void invoke("boot_mark", {
      stage: "raf_2",
      tMs: performance.now() - (window.__bootT0 ?? 0),
    }).catch(() => {});

    document.getElementById("boot-splash")?.remove();
    window.__bootMark?.("splash_removed");
    void invoke("boot_mark", {
      stage: "splash_removed",
      tMs: performance.now() - (window.__bootT0 ?? 0),
    }).catch(() => {});

    void invoke("frontend_ready", { phase: "splash_removed" }).catch(() => {});
    window.__bootMark?.("frontend_ready_sent_after_splash");
    void invoke("boot_mark", {
      stage: "frontend_ready_sent_after_splash",
      tMs: performance.now() - (window.__bootT0 ?? 0),
    }).catch(() => {});
  });
});

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const { settings } = useSettings();
  const [sessionThemeOverride, setSessionThemeOverride] =
    useState<EffectiveTheme | null>(null);
  const [systemTheme, setSystemTheme] = useState<EffectiveTheme>(bootTheme);
  const previousThemePreference = useRef<string | null>(null);

  const themePreference = normalizeThemePreference(
    settings?.theme ?? localStorage.getItem(THEME_PREF_KEY),
  );
  const accentPreset = normalizeAccentPreset(
    settings?.accent_preset ?? localStorage.getItem(ACCENT_PRESET_KEY),
  );

  useEffect(() => {
    if (
      previousThemePreference.current !== null &&
      previousThemePreference.current !== themePreference
    ) {
      setSessionThemeOverride(null);
    }
    previousThemePreference.current = themePreference;
  }, [themePreference]);

  useEffect(() => {
    if (themePreference !== "system") return;

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const syncSystemTheme = () => setSystemTheme(getSystemTheme());
    syncSystemTheme();
    media.addEventListener("change", syncSystemTheme);
    return () => media.removeEventListener("change", syncSystemTheme);
  }, [themePreference]);

  const persistedTheme =
    themePreference === "system" ? systemTheme : themePreference;
  const theme = sessionThemeOverride ?? persistedTheme;

  useEffect(() => {
    localStorage.setItem(THEME_PREF_KEY, themePreference);
    localStorage.setItem(ACCENT_PRESET_KEY, accentPreset);
    localStorage.setItem(THEME_EFFECTIVE_KEY, persistedTheme);
  }, [accentPreset, persistedTheme, themePreference]);

  useLayoutEffect(() => {
    applyAppearance(theme, accentPreset);
  }, [accentPreset, theme]);

  return (
    <ThemeContext.Provider
      value={{ theme, accentPreset, setTheme: setSessionThemeOverride }}
    >
      {children}
    </ThemeContext.Provider>
  );
}
