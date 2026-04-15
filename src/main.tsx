import React, { createContext, useState, useEffect } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./index.css";
import "./i18n";
import { LanguageContext } from "./context/LanguageContext";

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

const root = document.getElementById("root")!;
const bootTheme = resolveBootTheme();
root.setAttribute("data-theme", bootTheme);

export const ThemeContext = createContext<{
  theme: "dark" | "light";
  setTheme: (t: "dark" | "light") => void;
}>({ theme: bootTheme, setTheme: () => {} });

ReactDOM.createRoot(root).render(
  <React.StrictMode>
    <ThemeProvider>
      <LanguageProvider>
        <App />
      </LanguageProvider>
    </ThemeProvider>
  </React.StrictMode>
);

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<"dark" | "light">(bootTheme);
  const [themePref, setThemePref] = useState<ThemePref>(() =>
    normalizeThemePref(localStorage.getItem(THEME_PREF_KEY))
  );

  useEffect(() => {
    invoke<{ theme: string }>("get_settings")
      .then((settings) => {
        const pref = normalizeThemePref(settings.theme);
        const effective = resolveTheme(pref);
        setThemePref(pref);
        setTheme(effective);
      })
      .catch(() => {});
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
    invoke<{ language: string }>("get_settings")
      .then((settings) => {
        let lang = settings.language;
        // If language not yet set, detect from system
        if (!lang) {
          invoke<string>("get_system_locale").then((sysLang) => {
            lang = sysLang || "zh";
            setLanguageState(lang);
            import("./i18n").then((mod) => {
              mod.default.changeLanguage(lang);
            });
            // Persist the detected language to settings
            invoke("save_settings", {
              newSettings: { ...settings, language: lang }
            }).catch(() => {});
          }).catch(() => {
            lang = "zh";
            setLanguageState(lang);
            import("./i18n").then((mod) => mod.default.changeLanguage("zh"));
          });
        } else {
          setLanguageState(lang);
          import("./i18n").then((mod) => {
            mod.default.changeLanguage(lang);
          });
        }
      })
      .catch(() => {
        setLanguageState("zh");
        import("./i18n").then((mod) => mod.default.changeLanguage("zh"));
      });
  }, []);

  const setLanguage = (l: string) => {
    setLanguageState(l);
    import("./i18n").then((mod) => {
      mod.default.changeLanguage(l);
    });
  };

  return (
    <LanguageContext.Provider value={{ language: language || "zh", setLanguage }}>
      {children}
    </LanguageContext.Provider>
  );
}
