import React, { createContext, useContext, useState, useEffect } from "react";
import ReactDOM from "react-dom/client";
import { invoke } from "@tauri-apps/api/core";
import App from "./App";
import "./index.css";
import "./i18n";
import { LanguageContext } from "./context/LanguageContext";

export const ThemeContext = createContext<{
  theme: "dark" | "light";
  setTheme: (t: "dark" | "light") => void;
}>({ theme: "dark", setTheme: () => {} });

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <ThemeProvider>
      <LanguageProvider>
        <App />
      </LanguageProvider>
    </ThemeProvider>
  </React.StrictMode>
);

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setTheme] = useState<"dark" | "light">("dark");
  const [ready, setReady] = useState(false);

  useEffect(() => {
    invoke<{ theme: string }>("get_settings")
      .then((settings) => {
        if (settings.theme === "light" || settings.theme === "dark") {
          setTheme(settings.theme);
        } else if (settings.theme === "system") {
          const isDark = window.matchMedia("(prefers-color-scheme: dark)").matches;
          setTheme(isDark ? "dark" : "light");
        }
      })
      .catch(() => {})
      .finally(() => setReady(true));
  }, []);

  useEffect(() => {
    if (ready) {
      document.getElementById("root")!.setAttribute("data-theme", theme);
    }
  }, [theme, ready]);

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
