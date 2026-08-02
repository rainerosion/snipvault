import type { ReactNode } from "react";
import { useEffect, useState } from "react";
import { LanguageContext } from "./LanguageContext";
import i18n from "../i18n";

export interface LanguageProviderProps {
  children: ReactNode;
  loadInitialLanguage: () => Promise<string>;
}

function toDocumentLanguage(language: string): "en" | "zh-CN" {
  return language === "en" ? "en" : "zh-CN";
}

export function LanguageProvider({
  children,
  loadInitialLanguage,
}: LanguageProviderProps) {
  const [language, setLanguageState] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;

    loadInitialLanguage()
      .then((initialLanguage) => {
        if (cancelled) return;
        setLanguageState(initialLanguage);
        void i18n.changeLanguage(initialLanguage);
      })
      .catch(() => {
        if (cancelled) return;
        setLanguageState("zh");
        void i18n.changeLanguage("zh");
      });

    return () => {
      cancelled = true;
    };
  }, [loadInitialLanguage]);

  useEffect(() => {
    document.documentElement.lang = toDocumentLanguage(language ?? "zh");
  }, [language]);

  const setLanguage = (nextLanguage: string) => {
    setLanguageState(nextLanguage);
    void i18n.changeLanguage(nextLanguage);
  };

  return (
    <LanguageContext.Provider
      value={{ language: language ?? "zh", setLanguage }}
    >
      {children}
    </LanguageContext.Provider>
  );
}
