import React, { createContext, useState, useEffect } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./index.css";
import i18n from "./i18n";
import { LanguageContext } from "./context/LanguageContext";
import type { Settings as AppSettings } from "./hooks/useSettings";

declare global {
  interface Window {
    __bootT0?: number;
    __bootMark?: (stage: string) => void;
    __bootMarkToNative?: (stage: string) => void;
  }
}

const THEME_PREF_KEY = "snipvault-theme-pref";
const THEME_EFFECTIVE_KEY = "snipvault-theme-effective";
const THEME_PREF_EVENT = "snipvault-theme-pref-changed";
type ThemePref = "dark" | "light" | "system";

function getSystemTheme(): "dark" | "light" {
  return window.matchMedia("(prefers-color-scheme: dark)").matches ? "dark" : "light";
}

function normalizeThemePref(value: string | null | undefined): ThemePref {
  if (value === "dark" || value === "light" || value === "system") {
    return value;
  }
  return "system";
}

function resolveTheme(pref: ThemePref): "dark" | "light" {
  return pref === "system" ? getSystemTheme() : pref;
}

function resolveBootTheme(): "dark" | "light" {
  const pref = normalizeThemePref(localStorage.getItem(THEME_PREF_KEY));
  if (pref !== "system") return pref;

  const cached = localStorage.getItem(THEME_EFFECTIVE_KEY);
  if (cached === "dark" || cached === "light") {
    return cached;
  }

  return getSystemTheme();
}

window.__bootMarkToNative = (stage: string) => {
  const tMs = performance.now() - (window.__bootT0 ?? 0);
  window.__bootMark?.(stage);
  void invoke("boot_mark", { stage, tMs }).catch(() => {});
};

window.__bootMarkToNative("main_eval_start");

const root = document.getElementById("root")!;
const bootTheme = resolveBootTheme();
root.setAttribute("data-theme", bootTheme);

let bootSettingsPromise: Promise<AppSettings | null> | null = null;

function getBootSettings() {
  if (!bootSettingsPromise) {
    const t = performance.now() - (window.__bootT0 ?? 0);
    window.__bootMark?.("get_settings_start");
    void invoke("boot_mark", { stage: "get_settings_start", tMs: t }).catch(() => {});

    bootSettingsPromise = invoke<AppSettings>("get_settings")
      .then((settings) => {
        const done = performance.now() - (window.__bootT0 ?? 0);
        window.__bootMark?.("get_settings_done");
        void invoke("boot_mark", { stage: "get_settings_done", tMs: done }).catch(() => {});
        return settings;
      })
      .catch(() => {
        const fail = performance.now() - (window.__bootT0 ?? 0);
        window.__bootMark?.("get_settings_fail");
        void invoke("boot_mark", { stage: "get_settings_fail", tMs: fail }).catch(() => {});
        return null;
      });
  }
  return bootSettingsPromise;
}

export const ThemeContext = createContext<{
  theme: "dark" | "light";
  setTheme: (t: "dark" | "light") => void;
}>({ theme: bootTheme, setTheme: () => {} });

window.__bootMark?.("react_render_start");
void invoke("boot_mark", { stage: "react_render_start", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <ThemeProvider>
      <LanguageProvider>
        <App />
      </LanguageProvider>
    </ThemeProvider>
  </React.StrictMode>
);

window.__bootMark?.("react_render_called");
void invoke("boot_mark", { stage: "react_render_called", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});

void invoke("frontend_ready", { phase: "react_render_called" }).catch(() => {});
window.__bootMark?.("frontend_ready_sent");
void invoke("boot_mark", { stage: "frontend_ready_sent", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});

requestAnimationFrame(() => {
  window.__bootMark?.("raf_1");
  void invoke("boot_mark", { stage: "raf_1", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});

  requestAnimationFrame(() => {
    window.__bootMark?.("raf_2");
    void invoke("boot_mark", { stage: "raf_2", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});

    document.getElementById("boot-splash")?.remove();
    window.__bootMark?.("splash_removed");
    void invoke("boot_mark", { stage: "splash_removed", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});

    void invoke("frontend_ready", { phase: "splash_removed" }).catch(() => {});
    window.__bootMark?.("frontend_ready_sent_after_splash");
    void invoke("boot_mark", { stage: "frontend_ready_sent_after_splash", tMs: performance.now() - (window.__bootT0 ?? 0) }).catch(() => {});
  });
});

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<"dark" | "light">(bootTheme);
  const [themePref, setThemePref] = useState<ThemePref>(() =>
    normalizeThemePref(localStorage.getItem(THEME_PREF_KEY))
  );

  useEffect(() => {
    let cancelled = false;

    getBootSettings()
      .then((settings) => {
        if (cancelled || !settings) return;
        const pref = normalizeThemePref(settings.theme);
        const effective = resolveTheme(pref);
        setThemePref(pref);
        setTheme(effective);
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    const onThemePrefChanged = (event: Event) => {
      const detail = (event as CustomEvent<{ pref?: string; effective?: string }>).detail;
      const pref = normalizeThemePref(detail?.pref);
      const effective = detail?.effective === "dark" || detail?.effective === "light"
        ? detail.effective
        : resolveTheme(pref);
      setThemePref(pref);
      setTheme(effective);
    };

    window.addEventListener(THEME_PREF_EVENT, onThemePrefChanged);
    return () => {
      window.removeEventListener(THEME_PREF_EVENT, onThemePrefChanged);
    };
  }, []);

  useEffect(() => {
    if (themePref !== "system") return;

    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const onChange = (event: MediaQueryListEvent) => {
      setTheme(event.matches ? "dark" : "light");
    };

    media.addEventListener("change", onChange);
    return () => media.removeEventListener("change", onChange);
  }, [themePref]);

  useEffect(() => {
    localStorage.setItem(THEME_PREF_KEY, themePref);
  }, [themePref]);

  useEffect(() => {
    root.setAttribute("data-theme", theme);
    localStorage.setItem(THEME_EFFECTIVE_KEY, theme);
  }, [theme]);

  return (
    <ThemeContext.Provider value={{ theme, setTheme }}>
      {children}
    </ThemeContext.Provider>
  );
}

function LanguageProvider({ children }: { children: React.ReactNode }) {
  const [language, setLanguageState] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    getBootSettings()
      .then((settings) => {
        if (cancelled) return;

        const initialLang = settings?.language || "zh";
        setLanguageState(initialLang);
        void i18n.changeLanguage(initialLang);

        if (settings?.language) return;

        invoke<string>("get_system_locale")
          .then((sysLang) => {
            if (cancelled || !settings) return;
            const detected = sysLang || "zh";
            if (detected === initialLang) return;

            setLanguageState(detected);
            void i18n.changeLanguage(detected);
            invoke("save_settings", {
              newSettings: { ...settings, language: detected },
            }).catch(() => {});
          })
          .catch(() => {});
      })
      .catch(() => {
        if (cancelled) return;
        setLanguageState("zh");
        void i18n.changeLanguage("zh");
      });

    return () => {
      cancelled = true;
    };
  }, []);

  const setLanguage = (l: string) => {
    setLanguageState(l);
    void i18n.changeLanguage(l);
  };

  return (
    <LanguageContext.Provider value={{ language: language || "zh", setLanguage }}>
      {children}
    </LanguageContext.Provider>
  );
}
