import "./boot.css";
import {
  ACCENT_PRESET_KEY,
  applyAppearance,
  normalizeAccentPreset,
  resolveBootTheme,
} from "./theme";

declare global {
  interface Window {
    __bootT0?: number;
    __bootMark?: (stage: string) => void;
  }
}

window.__bootT0 = performance.now();
window.__bootMark = (stage: string) => {
  const elapsed = performance.now() - (window.__bootT0 ?? 0);
  console.info(`[BOOT][web] ${stage} +${elapsed.toFixed(2)}ms`);
};

window.__bootMark("boot_module_start");

const theme = resolveBootTheme();
const accentPreset = normalizeAccentPreset(
  localStorage.getItem(ACCENT_PRESET_KEY),
);
applyAppearance(theme, accentPreset);
window.__bootMark("theme_applied");
window.__bootMark("splash_dom_ready");
