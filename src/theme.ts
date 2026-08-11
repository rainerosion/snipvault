import type { AccentPreset, ThemePreference } from "./hooks/useSettings";

export const THEME_PREF_KEY = "snipvault-theme-pref";
export const THEME_EFFECTIVE_KEY = "snipvault-theme-effective";
export const ACCENT_PRESET_KEY = "snipvault-accent-preset";

export type EffectiveTheme = "dark" | "light";

export const ACCENT_PRESETS = [
  "sky",
  "violet",
  "emerald",
  "amber",
  "rose",
  "white",
] as const satisfies readonly AccentPreset[];

export function normalizeThemePreference(
  value: string | null | undefined,
): ThemePreference {
  if (value === "dark" || value === "light" || value === "system") {
    return value;
  }
  return "system";
}

export function normalizeAccentPreset(
  value: string | null | undefined,
): AccentPreset {
  return ACCENT_PRESETS.includes(value as AccentPreset)
    ? (value as AccentPreset)
    : "sky";
}

export function getSystemTheme(): EffectiveTheme {
  return window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

export function resolveThemePreference(
  preference: ThemePreference,
): EffectiveTheme {
  return preference === "system" ? getSystemTheme() : preference;
}

export function resolveBootTheme(): EffectiveTheme {
  const preference = normalizeThemePreference(
    localStorage.getItem(THEME_PREF_KEY),
  );
  if (preference !== "system") return preference;

  if (typeof window.matchMedia === "function") return getSystemTheme();

  const cached = localStorage.getItem(THEME_EFFECTIVE_KEY);
  return cached === "dark" || cached === "light" ? cached : "dark";
}

export function applyAppearance(
  theme: EffectiveTheme,
  accentPreset: AccentPreset,
): void {
  document.documentElement.setAttribute("data-theme", theme);
  document.documentElement.setAttribute("data-accent", accentPreset);

  const root = document.getElementById("root");
  root?.setAttribute("data-theme", theme);
  root?.setAttribute("data-accent", accentPreset);
}
