import "./boot.css";

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

const preference = localStorage.getItem("snipvault-theme-pref");
const cached = localStorage.getItem("snipvault-theme-effective");
let theme: "dark" | "light";
if (preference === "dark" || preference === "light") {
  theme = preference;
} else if (cached === "dark" || cached === "light") {
  theme = cached;
} else {
  theme = window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}
document.documentElement.setAttribute("data-theme", theme);
window.__bootMark("theme_applied");
window.__bootMark("splash_dom_ready");
