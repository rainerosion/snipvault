import { useCallback, useEffect, useMemo, useRef, useState, useContext } from "react";
import { useTranslation } from "react-i18next";
import CodeMirror from "@uiw/react-codemirror";
import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { githubDark, githubLight } from "@uiw/codemirror-theme-github";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { javascript } from "@codemirror/lang-javascript";
import { python } from "@codemirror/lang-python";
import { rust } from "@codemirror/lang-rust";
import { java } from "@codemirror/lang-java";
import { cpp } from "@codemirror/lang-cpp";
import { php } from "@codemirror/lang-php";
import { sql } from "@codemirror/lang-sql";
import { xml } from "@codemirror/lang-xml";
import { json } from "@codemirror/lang-json";
import { css } from "@codemirror/lang-css";
import { markdown } from "@codemirror/lang-markdown";
import { yaml } from "@codemirror/lang-yaml";
import { Snippet, SnippetForm } from "../types";
import { LANGUAGES } from "../utils/languages";
import { LanguageContext } from "../context/LanguageContext";

// ─── Syntax palettes ─────────────────────────────────────────────────────────

const darkHighlight = syntaxHighlighting(HighlightStyle.define([
  { tag: t.keyword, color: "#ff7b72" },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: "#ffa657" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "#d2a8ff" },
  { tag: [t.labelName], color: "#7ee787" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: "#ffa657" },
  { tag: [t.definition(t.name), t.separator], color: "#c9d1d9" },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: "#79c0ff" },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: "#ff7b72" },
  { tag: [t.meta, t.comment], color: "#8b949e", fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: "#a5d6ff", textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: "#79c0ff" },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: "#ff7b72" },
  { tag: [t.processingInstruction, t.string, t.inserted], color: "#a5d6ff" },
  { tag: t.number, color: "#79c0ff" },
  { tag: t.invalid, color: "#ff7b72" },
]));

const lightHighlight = syntaxHighlighting(HighlightStyle.define([
  { tag: t.keyword, color: "#cf222e" },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: "#953800" },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: "#8250df" },
  { tag: [t.labelName], color: "#116329" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: "#953800" },
  { tag: [t.definition(t.name), t.separator], color: "#24292f" },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: "#0550ae" },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: "#cf222e" },
  { tag: [t.meta, t.comment], color: "#6e7781", fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: "#0a3069", textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: "#0550ae" },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: "#cf222e" },
  { tag: [t.processingInstruction, t.string, t.inserted], color: "#0a3069" },
  { tag: t.number, color: "#0550ae" },
  { tag: t.invalid, color: "#cf222e" },
]));

// ─── Token → color mapping (for Canvas minimap) ─────────────────────────────

// Build a flat tag→color map from the HighlightStyle definitions
const DARK_TOKEN_COLORS: Record<string, string> = {
  keyword: "#ff7b72",
  name: "#ffa657",
  deleted: "#ffa657",
  character: "#ffa657",
  propertyName: "#ffa657",
  macroName: "#ffa657",
  function: "#d2a8ff",
  labelName: "#7ee787",
  color: "#ffa657",
  constant: "#ffa657",
  separator: "#c9d1d9",
  typeName: "#79c0ff",
  className: "#79c0ff",
  number: "#79c0ff",
  changed: "#79c0ff",
  annotation: "#79c0ff",
  modifier: "#79c0ff",
  self: "#79c0ff",
  namespace: "#79c0ff",
  operator: "#ff7b72",
  operatorKeyword: "#ff7b72",
  url: "#ff7b72",
  escape: "#ff7b72",
  regexp: "#ff7b72",
  link: "#ff7b72",
  meta: "#8b949e",
  comment: "#8b949e",
  atom: "#ff7b72",
  bool: "#ff7b72",
  processingInstruction: "#a5d6ff",
  string: "#a5d6ff",
  inserted: "#a5d6ff",
  invalid: "#ff7b72",
  heading: "#79c0ff",
  strong: "#c9d1d9",
  emphasis: "#c9d1d9",
  strikethrough: "#c9d1d9",
};

const LIGHT_TOKEN_COLORS: Record<string, string> = {
  keyword: "#cf222e",
  name: "#953800",
  deleted: "#953800",
  character: "#953800",
  propertyName: "#953800",
  macroName: "#953800",
  function: "#8250df",
  labelName: "#116329",
  color: "#953800",
  constant: "#953800",
  separator: "#24292f",
  typeName: "#0550ae",
  className: "#0550ae",
  number: "#0550ae",
  changed: "#0550ae",
  annotation: "#0550ae",
  modifier: "#0550ae",
  self: "#0550ae",
  namespace: "#0550ae",
  operator: "#cf222e",
  operatorKeyword: "#cf222e",
  url: "#cf222e",
  escape: "#cf222e",
  regexp: "#cf222e",
  link: "#cf222e",
  meta: "#6e7781",
  comment: "#6e7781",
  atom: "#cf222e",
  bool: "#cf222e",
  processingInstruction: "#0a3069",
  string: "#0a3069",
  inserted: "#0a3069",
  invalid: "#cf222e",
  heading: "#0550ae",
  strong: "#24292f",
  emphasis: "#24292f",
  strikethrough: "#24292f",
};

// Default line color when no token matches
const DEFAULT_DARK_LINE = "#30363d";
const DEFAULT_LIGHT_LINE = "#d0d7de";

// ─── Language extension ─────────────────────────────────────────────────────

function getLangExtension(lang: string) {
  switch (lang) {
    case "javascript": return javascript({ jsx: true, typescript: false });
    case "typescript": return javascript({ jsx: true, typescript: true });
    case "jsx": return javascript({ jsx: true });
    case "tsx": return javascript({ jsx: true, typescript: true });
    case "python": return python();
    case "rust": return rust();
    case "java": return java();
    case "cpp": case "c": case "csharp": return cpp();
    case "php": return php();
    case "sql": return sql();
    case "xml": case "html": return xml();
    case "json": return json();
    case "css": return css();
    case "markdown": return markdown();
    case "yaml": return yaml();
    default: return [];
  }
}

// ─── Main editor extensions ─────────────────────────────────────────────────

function buildMainExtensions(isDark: boolean, lang: string) {
  const selBg = isDark ? "rgba(56,189,248,0.62)" : "rgba(2,132,199,0.42)";
  const selBgF = isDark ? "rgba(56,189,248,0.74)" : "rgba(2,132,199,0.56)";
  const cursor = isDark ? "#38bdf8" : "#0284c7";
  const cmLayout = EditorView.theme({
    "&": {
      position: "absolute", top: "0", bottom: "0", left: "0", right: "0", fontSize: "13.5px",
    },
    ".cm-scroller": {
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
      overflowY: "auto !important", overflowX: "auto !important", height: "100%", display: "block",
    },
    ".cm-content": {
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
      caretColor: cursor,
    },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: cursor },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": { background: selBg },
    "&.cm-focused .cm-selectionBackground": { background: selBgF },
  });

  return [
    EditorView.lineWrapping,
    getLangExtension(lang),
    isDark ? githubDark : githubLight,
    cmLayout,
    isDark ? darkHighlight : lightHighlight,
  ];
}

// ─── Shadow DOM injection ────────────────────────────────────────────────────

function injectShadowStyles(isDark: boolean) {
  const bg = isDark ? "#0d1117" : "#ffffff";
  const selBg = isDark ? "rgba(56,189,248,0.62)" : "rgba(2,132,199,0.42)";
  const selBgF = isDark ? "rgba(56,189,248,0.74)" : "rgba(2,132,199,0.56)";
  const cursor = isDark ? "#38bdf8" : "#0284c7";
  const gutterBg = isDark ? "#0d1117" : "#f6f8fa";
  const gutterBorder = isDark ? "#21262d" : "#d0d7de";
  const gutterColor = isDark ? "#6e7681" : "#6e7781";
  const activeGutter = isDark ? "#161b22" : "#f6f8fa";
  const activeLine = isDark ? "rgba(56,189,248,0.05)" : "rgba(2,132,199,0.04)";
  return [
    `.cm-editor{background:${bg}!important;position:relative!important;}`,
    `.cm-scroller{background:${bg}!important;overflow-y:auto!important;overflow-x:auto!important;display:block!important;height:100%!important;}`,
    `.cm-content{caret-color:${cursor}!important;}`,
    `.cm-cursor,.cm-dropCursor{border-left-color:${cursor}!important;}`,
    `.cm-selectionLayer .cm-selectionBackground{background:${selBg}!important;opacity:1!important;}`,
    `.cm-focused .cm-selectionLayer .cm-selectionBackground{background:${selBgF}!important;opacity:1!important;}`,
    `.cm-content::selection,.cm-content *::selection{background:${selBg}!important;}`,
    `.cm-focused .cm-content::selection,.cm-focused .cm-content *::selection{background:${selBgF}!important;}`,
    `.cm-activeLine{background:${activeLine}!important;}`,
    `.cm-activeLineGutter{background:${activeGutter}!important;}`,
    `.cm-gutters{background:${gutterBg}!important;border-right:1px solid ${gutterBorder}!important;color:${gutterColor}!important;}`,
  ].join("");
}

// ─── MiniMap component (Canvas-based) ───────────────────────────────────────

interface MiniMapProps {
  content: string;
  isDark: boolean;
  width: number;
  // Called with the scroll container element so minimap can attach listeners
  onMainScrollRef: (el: HTMLElement | null) => void;
  // Scroll the main editor to a given scrollTop
  scrollMainTo: (scrollTop: number) => void;
  t?: (key: string) => string;
}

const GLANCE_LINE_H = 4; // line height in px in the minimap
const GLANCE_PADDING_X = 4; // horizontal padding inside minimap

// Color a line segment based on character type
function getColorByChar(ch: string, isDark: boolean): string {
  if (/[\s]/.test(ch)) return isDark ? DEFAULT_DARK_LINE : DEFAULT_LIGHT_LINE;
  if (/[{}()\[\]]/.test(ch)) return isDark ? "#ff7b72" : "#cf222e";
  if (/[=+\-*/<>!&|?:;,.]/.test(ch)) return isDark ? "#ff7b72" : "#cf222e";
  if (/['"`]/.test(ch)) return isDark ? "#a5d6ff" : "#0a3069";
  if (/[0-9]/.test(ch)) return isDark ? "#79c0ff" : "#0550ae";
  if (/[A-Z]/.test(ch)) return isDark ? "#79c0ff" : "#0550ae";
  return isDark ? "#c9d1d9" : "#24292f";
}

function tokenizeForMinimap(lineText: string, isDark: boolean): { text: string; color: string }[] {
  if (!lineText.trim()) return [];
  const segments: { text: string; color: string }[] = [];
  let currentColor = "";
  let currentText = "";
  for (let i = 0; i < lineText.length; i++) {
    const ch = lineText[i];
    const color = getColorByChar(ch, isDark);
    if (color !== currentColor) {
      if (currentText) segments.push({ text: currentText, color: currentColor });
      currentColor = color;
      currentText = ch;
    } else {
      currentText += ch;
    }
  }
  if (currentText) segments.push({ text: currentText, color: currentColor });
  return segments.length > 0 ? segments : [{ text: lineText, color: isDark ? DEFAULT_DARK_LINE : DEFAULT_LIGHT_LINE }];
}

function MiniMap({ content, isDark, width, onMainScrollRef, scrollMainTo, t }: MiniMapProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const paneRef = useRef<HTMLDivElement>(null);
  const mainRef = useRef<HTMLElement | null>(null);
  const vpTopRef = useRef(0);
  const vpHRef = useRef(0);

  const bg = isDark ? "#0d1117" : "#ffffff";
  const viewportBg = isDark ? "rgba(56,189,248,0.12)" : "rgba(2,132,199,0.10)";
  const viewportBorder = isDark ? "rgba(56,189,248,0.45)" : "rgba(2,132,199,0.40)";
  const cursorLineBg = isDark ? "rgba(56,189,248,0.15)" : "rgba(2,132,199,0.12)";
  const cursorLineBorder = isDark ? "rgba(56,189,248,0.6)" : "rgba(2,132,199,0.55)";

  // Lines derived once per content change
  const lines = content.split("\n");
  const totalH = lines.length * GLANCE_LINE_H;

  // Compute max line length for scaling
  const maxLen = lines.reduce((m, l) => Math.max(m, l.length), 1);
  const availW = width - GLANCE_PADDING_X * 2;
  const scaleX = availW / maxLen;

  // Draw the minimap canvas
  const drawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    const pane = paneRef.current;
    if (!canvas || !pane) return;

    const paneH = pane.clientHeight;

    canvas.width = width;
    canvas.height = Math.max(totalH, paneH);
    const ctx = canvas.getContext("2d")!;
    ctx.clearRect(0, 0, width, canvas.height);

    lines.forEach((lineText, i) => {
      const y = i * GLANCE_LINE_H;
      ctx.fillStyle = bg;
      ctx.fillRect(0, y, width, GLANCE_LINE_H);
      if (!lineText.trim()) return;

      const segments = tokenizeForMinimap(lineText, isDark);
      let x = GLANCE_PADDING_X;

      for (const seg of segments) {
        const segW = Math.min(seg.text.length * scaleX, width - x - 1);
        if (segW > 0.5) {
          ctx.fillStyle = seg.color;
          ctx.beginPath();
          ctx.roundRect(x, y + 0.5, segW, GLANCE_LINE_H - 1, 1);
          ctx.fill();
          x += segW;
          if (x >= width - GLANCE_PADDING_X) break;
        }
      }
    });
  }, [lines, width, bg, isDark, totalH, scaleX]);

  // Draw + update viewport
  const updateViewport = useCallback(() => {
    const vp = viewportRef.current;
    const pane = paneRef.current;
    const main = mainRef.current;
    if (!vp || !pane) return;

    const paneH = pane.clientHeight;

    if (!main || main.scrollHeight <= main.clientHeight) {
      vp.style.display = "none";
      vpTopRef.current = 0;
      vpHRef.current = 0;
      return;
    }

    const mainScrollable = main.scrollHeight - main.clientHeight;
    const ratio = main.scrollTop / mainScrollable;
    const vpTotalH = Math.max(totalH, paneH);
    const vpScrollable = vpTotalH - paneH;
    if (vpScrollable <= 0) { vp.style.display = "none"; return; }

    const vpH = Math.max(24, (main.clientHeight / main.scrollHeight) * vpTotalH);
    const vpTop = ratio * vpScrollable;

    vp.style.display = "";
    vp.style.top = `${vpTop}px`;
    vp.style.height = `${vpH}px`;
    vpTopRef.current = vpTop;
    vpHRef.current = vpH;
  }, [totalH]);

  // ── Attach to main scroll container via callback ref ───────────────────────
  const setMainRef = useCallback((el: HTMLElement | null) => {
    if (mainRef.current === el) return;
    if (mainRef.current) {
      mainRef.current.removeEventListener("scroll", onMainScroll);
    }
    mainRef.current = el;
    onMainScrollRef(el);
    if (el) {
      el.addEventListener("scroll", onMainScroll, { passive: true });
      updateViewport();
    }
  }, [onMainScrollRef, updateViewport]);

  const onMainScroll = useCallback(() => {
    updateViewport();
  }, [updateViewport]);

  // ── ResizeObserver: redraw when pane resizes ──────────────────────────────
  useEffect(() => {
    const pane = paneRef.current;
    if (!pane) return;
    const ro = new ResizeObserver(() => { drawCanvas(); updateViewport(); });
    ro.observe(pane);
    return () => ro.disconnect();
  }, [drawCanvas, updateViewport]);

  // ── Redraw on content/theme/width change ──────────────────────────────────
  useEffect(() => { drawCanvas(); }, [drawCanvas]);
  useEffect(() => { updateViewport(); }, [updateViewport]);

  // ── Click to jump ──────────────────────────────────────────────────────────
  const handleClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    const main = mainRef.current;
    const pane = paneRef.current;
    if (!main || !pane) return;

    const rect = pane.getBoundingClientRect();
    const clickY = e.clientY - rect.top;
    const paneH = pane.clientHeight;
    const vpTotalH = Math.max(totalH, paneH);
    const vpScrollable = vpTotalH - paneH;

    if (vpScrollable <= 0) return;

    // Map click Y → line index → scroll main editor
    const lineIndex = Math.floor((clickY / paneH) * lines.length);
    const clampedLine = Math.max(0, Math.min(lines.length - 1, lineIndex));

    // Scroll main: use line height approximation
    const mainScrollable = main.scrollHeight - main.clientHeight;
    const targetRatio = clampedLine / lines.length;
    const targetScrollTop = targetRatio * mainScrollable;
    scrollMainTo(targetScrollTop);
  }, [lines, totalH, scrollMainTo]);

  // ── Hover: show cursor line indicator ────────────────────────────────────
  const [hoverLine, setHoverLine] = useState(-1);

  const handleMouseMove = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    const pane = paneRef.current;
    if (!pane) return;
    const rect = pane.getBoundingClientRect();
    const y = e.clientY - rect.top;
    const line = Math.floor(y / GLANCE_LINE_H);
    setHoverLine(line);
  }, []);

  const handleMouseLeave = useCallback(() => { setHoverLine(-1); }, []);

  // Draw hover line indicator on top of canvas
  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    if (hoverLine < 0 || hoverLine >= lines.length) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;
    const y = hoverLine * GLANCE_LINE_H;
    ctx.save();
    ctx.fillStyle = cursorLineBg;
    ctx.strokeStyle = cursorLineBorder;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(0.5, y + 0.5, width - 1, GLANCE_LINE_H - 1, 1);
    ctx.fill();
    ctx.stroke();
    ctx.restore();
  }, [hoverLine, lines.length, width, cursorLineBg, cursorLineBorder]);

  return (
    <div
      ref={paneRef}
      className="minimap-pane"
      style={{ width, background: bg }}
      onClick={handleClick}
      onMouseMove={handleMouseMove}
      onMouseLeave={handleMouseLeave}
      title={t ? t("minimap.clickToJump") : "点击跳转"}
    >
      {/* Canvas: drawn underneath */}
      <canvas
        ref={canvasRef}
        className="minimap-canvas"
        style={{ display: "block" }}
      />
      {/* Viewport overlay: shows current visible region */}
      <div
        ref={viewportRef}
        className="minimap-viewport"
        style={{ background: viewportBg, borderColor: viewportBorder }}
      />
      {/* Hover cursor line indicator */}
      {hoverLine >= 0 && (
        <div
          className="minimap-hover-line"
          style={{
            position: "absolute",
            left: 0,
            right: 0,
            top: hoverLine * GLANCE_LINE_H,
            height: GLANCE_LINE_H,
            background: cursorLineBg,
            borderTop: `1px solid ${cursorLineBorder}`,
            borderBottom: `1px solid ${cursorLineBorder}`,
            pointerEvents: "none",
          }}
        />
      )}
    </div>
  );
}

// ─── Component ──────────────────────────────────────────────────────────────

export function SnippetEditor({
  snippet, isNew, form, onChange, onSave, onCancel,
  theme, saving, isDirty, tagOptions,
}: SnippetEditorProps) {
  const { t } = useTranslation();
  const { language } = useContext(LanguageContext);

  // ── Refs ──────────────────────────────────────────────────────────────────
  const editorWrapRef = useRef<HTMLDivElement>(null);
  const cmRef = useRef<EditorView | null>(null);
  const mainScrollerRef = useRef<HTMLElement | null>(null);

  // ── State ─────────────────────────────────────────────────────────────────
  const [copied, setCopied] = useState(false);
  const [tagInput, setTagInput] = useState("");
  const [showTagSuggestions, setShowTagSuggestions] = useState(false);
  const [minimapWidth, setMinimapWidth] = useState(120);
  const [isDragging, setIsDragging] = useState(false);
  const [minimapKey, setMinimapKey] = useState(0); // force re-mount when scroller changes

  // ── Derived ───────────────────────────────────────────────────────────────
  const isDark = theme === "dark";
  const mainExtensions = useMemo(
    () => buildMainExtensions(isDark, form.language),
    [isDark, form.language]
  );

  // ── Scroll main editor to a given scrollTop ────────────────────────────────
  const scrollMainTo = useCallback((scrollTop: number) => {
    if (cmRef.current) {
      cmRef.current.scrollDOM.scrollTop = scrollTop;
    }
  }, []);

  // ── Minimap scroll-el callback: store it and bump key so MiniMap re-inits ──
  const handleMinimapScrollRef = useCallback((el: HTMLElement | null) => {
    if (mainScrollerRef.current !== el) {
      mainScrollerRef.current = el;
      setMinimapKey((k) => k + 1);
    }
  }, []);

  // ── Main editor height fixup ───────────────────────────────────────────────
  useEffect(() => {
    const wrap = editorWrapRef.current;
    if (!wrap) return;
    const apply = () => {
      const view = cmRef.current;
      if (!view) return;
      const px = wrap.clientHeight;
      if (px <= 0) return;
      (view.dom as HTMLElement).style.height = `${px}px`;
      const sr = (view.dom as HTMLElement).shadowRoot;
      if (!sr) return;
      let s = sr.querySelector("[data-snpt]") as HTMLStyleElement | null;
      if (!s) { s = document.createElement("style"); s.setAttribute("data-snpt", ""); sr.appendChild(s); }
      s.textContent = injectShadowStyles(isDark);
      const sc = sr.querySelector(".cm-scroller") as HTMLElement | null;
      if (sc) {
        sc.style.overflowY = "auto";
        sc.style.overflowX = "auto";
        sc.style.height = "100%";
      }
    };
    apply();
    requestAnimationFrame(apply);
    const ro = new ResizeObserver(apply);
    ro.observe(wrap);
    return () => ro.disconnect();
  }, [theme]);

  // ── Minimap divider drag ───────────────────────────────────────────────────
  const handleDividerPointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    setIsDragging(true);
    const startX = e.clientX;
    const startWidth = minimapWidth;

    const onMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX;
      setMinimapWidth((w) => Math.min(420, Math.max(80, startWidth + delta)));
    };
    const onUp = () => {
      setIsDragging(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }, [minimapWidth]);

  // ── Tag helpers ────────────────────────────────────────────────────────────
  const filteredTagOptions = useMemo(() => {
    const q = tagInput.trim().toLowerCase();
    return tagOptions.filter((tag) => !form.tags.includes(tag) && (q === "" || tag.toLowerCase().includes(q))).slice(0, 8);
  }, [tagOptions, form.tags, tagInput]);

  const addTag = useCallback((raw: string) => {
    const tag = raw.trim();
    if (!tag) return;
    if (form.tags.includes(tag)) { setTagInput(""); setShowTagSuggestions(false); return; }
    onChange({ tags: [...form.tags, tag] });
    setTagInput(""); setShowTagSuggestions(false);
  }, [form.tags, onChange]);

  const removeTag = useCallback((tag: string) => onChange({ tags: form.tags.filter((t) => t !== tag) }), [form.tags, onChange]);
  const commitTagInput = useCallback(() => addTag(tagInput), [addTag, tagInput]);

  const handleCopy = useCallback(async () => {
    if (!form.content) return;
    try {
      await writeText(form.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) { console.error("Copy failed:", e); }
  }, [form.content]);

  const formatDate = (iso: string) => {
    try { return new Date(iso).toLocaleString(language === "zh" ? "zh-CN" : "en-US", { hour12: false }); }
    catch { return iso; }
  };

  // ── Empty state ───────────────────────────────────────────────────────────
  if (!snippet && !isNew) {
    return (
      <div className="editor-empty">
        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
          <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z"/>
        </svg>
        <p>{t("snippet.selectHint")}</p>
        <p className="hint">{t("snippet.shortcutHint")}</p>
      </div>
    );
  }

  // ── Render ────────────────────────────────────────────────────────────────
  return (
    <div className="editor-form">
      {/* Header */}
      <div className="editor-header">
        <input
          className="title-input"
          placeholder={t("snippet.title")}
          value={form.title}
          onChange={(e) => onChange({ title: e.target.value })}
          autoFocus={isNew}
        />
        <div className="editor-header-right">
          {isDirty && <span className="unsaved-dot" title={t("snippet.unsaved")} />}
          <select
            className="lang-select-sm"
            value={form.language}
            onChange={(e) => onChange({ language: e.target.value })}
          >
            {LANGUAGES.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
          </select>
          <button
            className={`fav-toggle ${form.is_favorite ? "active" : ""}`}
            onClick={() => onChange({ is_favorite: !form.is_favorite })}
            title={form.is_favorite ? t("snippet.unfavorite") : t("snippet.favorite")}
          >
            <svg width="16" height="16" viewBox="0 0 24 24"
              fill={form.is_favorite ? "currentColor" : "none"}
              stroke="currentColor" strokeWidth="2">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
            </svg>
          </button>
        </div>
      </div>

      {/* Description */}
      <input
        className="desc-input"
        placeholder={t("snippet.desc")}
        value={form.description}
        onChange={(e) => onChange({ description: e.target.value })}
      />

      {/* Tags */}
      <div className="tags-row">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z"/>
          <line x1="7" y1="7" x2="7.01" y2="7"/>
        </svg>
        <div className="editor-tags">
          {form.tags.map((tag) => (
            <span key={tag} className="tag-chip">
              <span>{tag}</span>
              <button type="button" className="tag-remove" onClick={() => removeTag(tag)} title={t("snippet.delete")}>×</button>
            </span>
          ))}
          <div className="tag-input-wrap">
            <input
              className="tag-input"
              placeholder={t("snippet.tags")}
              value={tagInput}
              onFocus={() => setShowTagSuggestions(true)}
              onBlur={() => setTimeout(() => setShowTagSuggestions(false), 120)}
              onChange={(e) => { setTagInput(e.target.value); setShowTagSuggestions(true); }}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === ",") { e.preventDefault(); commitTagInput(); }
                if (e.key === "Backspace" && !tagInput && form.tags.length > 0) removeTag(form.tags[form.tags.length - 1]);
              }}
            />
            {showTagSuggestions && filteredTagOptions.length > 0 && (
              <div className="tag-suggestions">
                {filteredTagOptions.map((tag) => (
                  <button key={tag} type="button" className="tag-suggestion"
                    onMouseDown={(e) => { e.preventDefault(); addTag(tag); }}>
                    {tag}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      {/* Editor + MiniMap split */}
      <div className="cm-editor-split">
        {/* Main editor */}
        <div className="cm-main-pane" ref={editorWrapRef}>
          <CodeMirror
            value={form.content}
            extensions={mainExtensions}
            onChange={(val) => {
              onChange({ content: val });
            }}
            onCreateEditor={(view) => {
              cmRef.current = view;
              const sr = (view.dom as HTMLElement).shadowRoot;
              if (!sr) return;
              let s = sr.querySelector("[data-snpt]") as HTMLStyleElement | null;
              if (!s) { s = document.createElement("style"); s.setAttribute("data-snpt", ""); sr.appendChild(s); }
              s.textContent = injectShadowStyles(isDark);
              const sc = sr.querySelector(".cm-scroller") as HTMLElement | null;
              if (sc) {
                sc.style.overflowY = "auto";
                sc.style.overflowX = "auto";
                sc.style.height = "100%";
                mainScrollerRef.current = sc;
              }
            }}
            basicSetup={{
              lineNumbers: true, drawSelection: true, highlightActiveLine: true,
              highlightSelectionMatches: true, autocompletion: true,
              bracketMatching: true, closeBrackets: true, foldGutter: true, indentOnInput: true,
            }}
          />
        </div>

        {/* Draggable divider */}
        <div
          className={`codeglance-divider ${isDragging ? "active" : ""}`}
          onPointerDown={handleDividerPointerDown}
        />

        {/* MiniMap */}
        <MiniMap
          key={`${minimapWidth}-${form.content.length}-${minimapKey}`}
          content={form.content}
          isDark={isDark}
          width={minimapWidth}
          onMainScrollRef={handleMinimapScrollRef}
          scrollMainTo={scrollMainTo}
          t={t}
        />
      </div>

      {/* Toolbar */}
      <div className="editor-toolbar">
        <div className="footer-hint">
          {snippet && (
            <span className={`last-saved ${isDirty ? "unsaved-text" : ""}`}>
              {isDirty ? t("snippet.unsaved") : t("snippet.savedAt", { time: formatDate(snippet.updated_at) })}
            </span>
          )}
          <span className="shortcut-hint">{t("snippet.shortcutSave")}</span>
        </div>
        <div className="footer-btns">
          {form.content && (
            <button className={`btn-copy ${copied ? "copied" : ""}`} onClick={handleCopy} title={t("snippet.copy")}>
              {copied ? t("snippet.copied") : t("snippet.copy")}
            </button>
          )}
          {(isNew || isDirty) && (
            <button className="btn-cancel" onClick={onCancel}>
              {isNew ? t("snippet.cancel") : t("snippet.cancelEdit")}
            </button>
          )}
          <button className="btn-save" onClick={onSave} disabled={saving}>
            {saving ? t("snippet.saveInProgress") : isNew ? t("snippet.create") : t("snippet.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
