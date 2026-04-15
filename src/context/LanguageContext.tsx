import { createContext } from "react";

export const LanguageContext = createContext<{
  language: string;
  setLanguage: (l: string) => void;
}>({ language: "zh", setLanguage: () => {} });
