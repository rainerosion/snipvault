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

interface SnippetEditorProps {
  snippet: Snippet | null;
  isNew: boolean;
  form: SnippetForm;
  onChange: (f: Partial<SnippetForm>) => void;
  onSave: () => void;
  onCancel: () => void;
  theme: "dark" | "light";
  saving: boolean;
  isDirty: boolean;
  tagOptions: string[];
}

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

function getLangExtension(lang: string) {
  switch (lang) {
    case "javascript": return javascript({ jsx: true, typescript: false });
    case "typescript": return javascript({ jsx: true, typescript: true });
    case "jsx": return javascript({ jsx: true });
    case "tsx": return javascript({ jsx: true, typescript: true });
    case "python": return python();
    case "rust": return rust();
    case "java": return java();
    case "cpp":
    case "c":
    case "csharp": return cpp();
    case "php": return php();
    case "sql": return sql();
    case "xml":
    case "html": return xml();
    case "json": return json();
    case "css": return css();
    case "markdown": return markdown();
    case "yaml": return yaml();
    default: return [];
  }
}

function buildMainExtensions(isDark: boolean, lang: string) {
  const selBg = isDark ? "rgba(56,189,248,0.62)" : "rgba(2,132,199,0.42)";
  const selBgF = isDark ? "rgba(56,189,248,0.74)" : "rgba(2,132,199,0.56)";
  const cursor = isDark ? "#38bdf8" : "#0284c7";

  const cmLayout = EditorView.theme({
    "&": {
      height: "100%",
      minHeight: "0",
      fontSize: "13.5px",
    },
    ".cm-scroller": {
      height: "100%",
      minHeight: "0",
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
      overflowY: "auto !important",
      overflowX: "auto !important",
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

interface MiniMapProps {
  content: string;
  isDark: boolean;
  width: number;
  mainScrollEl: HTMLElement | null;
  scrollMainTo: (scrollTop: number) => void;
}

const GLANCE_LINE_H = 4;
const GLANCE_PADDING_X = 4;

function getColorByChar(ch: string, isDark: boolean): string {
  if (/[\s]/.test(ch)) return isDark ? "#30363d" : "#d0d7de";
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
  return segments;
}

function MiniMap({ content, isDark, width, mainScrollEl, scrollMainTo }: MiniMapProps) {
  const paneRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const syncingFromMiniRef = useRef(false);
  const draggingViewportRef = useRef<{ pointerId: number; pointerOffsetInViewport: number } | null>(null);
  const suppressClickRef = useRef(false);

  const [isViewportDragging, setIsViewportDragging] = useState(false);
  const [paneClientHeight, setPaneClientHeight] = useState(0);
  const [mainCanScroll, setMainCanScroll] = useState(false);

  const bg = isDark ? "#0d1117" : "#ffffff";
  const viewportBg = isDark ? "rgba(56,189,248,0.26)" : "rgba(2,132,199,0.22)";
  const viewportBorder = isDark ? "rgba(56,189,248,0.95)" : "rgba(2,132,199,0.90)";

  const lines = useMemo(() => content.split("\n"), [content]);
  const lineContentH = useMemo(() => Math.max(lines.length * GLANCE_LINE_H, 1), [lines.length]);
  const totalH = useMemo(() => {
    if (mainCanScroll && lineContentH <= paneClientHeight) {
      return paneClientHeight + 1;
    }
    return lineContentH;
  }, [mainCanScroll, lineContentH, paneClientHeight]);
  const maxLen = useMemo(() => Math.max(...lines.map((l) => l.length), 1), [lines]);
  const availW = Math.max(1, width - GLANCE_PADDING_X * 2);
  const scaleX = availW / maxLen;

  const drawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    canvas.width = width;
    canvas.height = totalH;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, width, totalH);

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
  }, [lines, width, totalH, bg, isDark, scaleX]);

  const syncViewportFromMain = useCallback(() => {
    const pane = paneRef.current;
    const viewport = viewportRef.current;
    const main = mainScrollEl;
    if (!pane || !viewport || !main) return;

    const canScroll = main.scrollHeight > main.clientHeight + 0.5;
    setPaneClientHeight(pane.clientHeight);
    setMainCanScroll(canScroll);

    const mainScrollable = Math.max(0, main.scrollHeight - main.clientHeight);
    const ratio = mainScrollable > 0 ? main.scrollTop / mainScrollable : 0;

    const miniScrollable = Math.max(0, pane.scrollHeight - pane.clientHeight);
    if (miniScrollable > 0 && !syncingFromMiniRef.current) {
      pane.scrollTop = ratio * miniScrollable;
    }

    if (!canScroll) {
      viewport.style.display = "none";
      return;
    }

    const vpH = Math.max(24, Math.min(180, (main.clientHeight / main.scrollHeight) * totalH));
    const vpTop = ratio * Math.max(0, totalH - vpH);

    viewport.style.display = "";
    viewport.style.height = `${vpH}px`;
    viewport.style.top = `${vpTop}px`;
  }, [mainScrollEl, totalH]);

  useEffect(() => {
    drawCanvas();
    syncViewportFromMain();
  }, [drawCanvas, syncViewportFromMain]);

  useEffect(() => {
    const pane = paneRef.current;
    if (!pane) return;
    const ro = new ResizeObserver(() => syncViewportFromMain());
    ro.observe(pane);
    return () => ro.disconnect();
  }, [syncViewportFromMain]);

  useEffect(() => {
    const main = mainScrollEl;
    if (!main) return;
    const onMainScroll = () => {
      syncViewportFromMain();
    };
    main.addEventListener("scroll", onMainScroll, { passive: true });
    syncViewportFromMain();
    return () => main.removeEventListener("scroll", onMainScroll);
  }, [mainScrollEl, syncViewportFromMain]);

  const handleClick = useCallback((e: React.MouseEvent<HTMLDivElement>) => {
    if (suppressClickRef.current) {
      suppressClickRef.current = false;
      return;
    }

    const pane = paneRef.current;
    const main = mainScrollEl;
    if (!pane || !main) return;

    const rect = pane.getBoundingClientRect();
    const clickY = e.clientY - rect.top;
    const mainScrollable = Math.max(0, main.scrollHeight - main.clientHeight);
    if (mainScrollable <= 0) {
      scrollMainTo(0);
      syncViewportFromMain();
      return;
    }

    const vpH = Math.max(24, Math.min(180, (main.clientHeight / main.scrollHeight) * totalH));
    const visibleTrackH = Math.max(1, pane.clientHeight - Math.min(vpH, pane.clientHeight));
    const ratio = Math.max(0, Math.min(1, (clickY - vpH / 2) / visibleTrackH));
    scrollMainTo(ratio * mainScrollable);

    syncViewportFromMain();
  }, [mainScrollEl, scrollMainTo, syncViewportFromMain, totalH]);

  const handleViewportPointerDown = useCallback((e: React.PointerEvent<HTMLDivElement>) => {
    const main = mainScrollEl;
    const pane = paneRef.current;
    const viewport = viewportRef.current;
    if (!main || !pane || !viewport) return;
    e.preventDefault();
    e.stopPropagation();
    suppressClickRef.current = false;

    const viewportRect = viewport.getBoundingClientRect();
    const pointerOffsetInViewport = Math.max(0, Math.min(viewportRect.height, e.clientY - viewportRect.top));

    draggingViewportRef.current = {
      pointerId: e.pointerId,
      pointerOffsetInViewport,
    };
    setIsViewportDragging(true);
    e.currentTarget.setPointerCapture(e.pointerId);
  }, [mainScrollEl]);

  useEffect(() => {
    const onPointerMove = (ev: PointerEvent) => {
      const drag = draggingViewportRef.current;
      const main = mainScrollEl;
      const pane = paneRef.current;
      if (!drag || !main || !pane || ev.pointerId !== drag.pointerId) return;

      suppressClickRef.current = true;

      const mainScrollable = Math.max(0, main.scrollHeight - main.clientHeight);
      if (mainScrollable <= 0) {
        scrollMainTo(0);
        pane.scrollTop = 0;
        syncViewportFromMain();
        return;
      }

      const vpH = Math.max(24, Math.min(180, (main.clientHeight / main.scrollHeight) * totalH));
      const visibleTrackH = Math.max(1, pane.clientHeight - Math.min(vpH, pane.clientHeight));
      const pointerYInPane = ev.clientY - pane.getBoundingClientRect().top;
      const viewportTopInPane = pointerYInPane - drag.pointerOffsetInViewport;
      const nextRatio = Math.max(0, Math.min(1, viewportTopInPane / visibleTrackH));
      const targetTop = nextRatio * mainScrollable;

      syncingFromMiniRef.current = true;
      scrollMainTo(targetTop);

      const miniScrollable = Math.max(0, pane.scrollHeight - pane.clientHeight);
      if (miniScrollable > 0) {
        pane.scrollTop = nextRatio * miniScrollable;
      }

      syncViewportFromMain();
      requestAnimationFrame(() => { syncingFromMiniRef.current = false; });
    };

    const endDrag = (ev: PointerEvent) => {
      const drag = draggingViewportRef.current;
      if (!drag || ev.pointerId !== drag.pointerId) return;
      draggingViewportRef.current = null;
      setIsViewportDragging(false);
    };

    window.addEventListener("pointermove", onPointerMove);
    window.addEventListener("pointerup", endDrag);
    window.addEventListener("pointercancel", endDrag);

    return () => {
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", endDrag);
      window.removeEventListener("pointercancel", endDrag);
    };
  }, [mainScrollEl, scrollMainTo, syncViewportFromMain, totalH]);

  return (
    <div className="minimap-wrap" style={{ width }}>
      <div
        ref={paneRef}
        className="minimap-pane"
        style={{ background: bg }}
        onClick={handleClick}
      >
        <div className="minimap-content" style={{ height: totalH, minHeight: "100%" }}>
          <canvas ref={canvasRef} className="minimap-canvas" style={{ display: "block" }} />

          <div
            ref={viewportRef}
            className={`minimap-viewport ${isViewportDragging ? "dragging" : ""}`}
            style={{ background: viewportBg, borderColor: viewportBorder }}
            onPointerDown={handleViewportPointerDown}
          />
        </div>
      </div>

    </div>
  );
}

export function SnippetEditor({
  snippet, isNew, form, onChange, onSave, onCancel,
  theme, saving, isDirty, tagOptions,
}: SnippetEditorProps) {
  const { t } = useTranslation();
  const { language } = useContext(LanguageContext);

  const cmRef = useRef<EditorView | null>(null);
  const [mainScrollEl, setMainScrollEl] = useState<HTMLElement | null>(null);

  const splitRef = useRef<HTMLDivElement | null>(null);
  const [copied, setCopied] = useState(false);
  const [tagInput, setTagInput] = useState("");
  const [showTagSuggestions, setShowTagSuggestions] = useState(false);
  const [minimapWidth, setMinimapWidth] = useState(120);
  const [isDragging, setIsDragging] = useState(false);

  const isDark = theme === "dark";
  const mainExtensions = useMemo(
    () => buildMainExtensions(isDark, form.language),
    [isDark, form.language]
  );

  const scrollMainTo = useCallback((scrollTop: number) => {
    if (cmRef.current) {
      cmRef.current.scrollDOM.scrollTop = scrollTop;
    }
  }, []);

  const handleDividerPointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    setIsDragging(true);
    const startX = e.clientX;
    const startWidth = minimapWidth;

    const onMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX;
      const next = startWidth + delta;
      const splitWidth = splitRef.current?.clientWidth ?? 0;
      const dynamicMax = splitWidth > 0 ? Math.floor(splitWidth * 0.5) : 420;
      const maxWidth = Math.max(120, Math.min(420, dynamicMax));
      setMinimapWidth(Math.min(maxWidth, Math.max(120, next)));
    };
    const onUp = () => {
      setIsDragging(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }, [minimapWidth]);

  const filteredTagOptions = useMemo(() => {
    const q = tagInput.trim().toLowerCase();
    return tagOptions
      .filter((tag) => !form.tags.includes(tag) && (q === "" || tag.toLowerCase().includes(q)))
      .slice(0, 8);
  }, [tagOptions, form.tags, tagInput]);

  const addTag = useCallback((raw: string) => {
    const tag = raw.trim();
    if (!tag) return;
    if (form.tags.includes(tag)) { setTagInput(""); setShowTagSuggestions(false); return; }
    onChange({ tags: [...form.tags, tag] });
    setTagInput("");
    setShowTagSuggestions(false);
  }, [form.tags, onChange]);

  const removeTag = useCallback((tag: string) => {
    onChange({ tags: form.tags.filter((x) => x !== tag) });
  }, [form.tags, onChange]);

  const commitTagInput = useCallback(() => addTag(tagInput), [addTag, tagInput]);

  const handleCopy = useCallback(async () => {
    if (!form.content) return;
    try {
      await writeText(form.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      console.error("Copy failed:", e);
    }
  }, [form.content]);

  const formatDate = (iso: string) => {
    try {
      return new Date(iso).toLocaleString(language === "zh" ? "zh-CN" : "en-US", { hour12: false });
    } catch {
      return iso;
    }
  };

  if (!snippet && !isNew) {
    return (
      <div className="editor-empty">
        <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
          <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
        </svg>
        <p>{t("snippet.selectHint")}</p>
        <p className="hint">{t("snippet.shortcutHint")}</p>
      </div>
    );
  }

  return (
    <div className="editor-form">
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
          <select className="lang-select-sm" value={form.language} onChange={(e) => onChange({ language: e.target.value })}>
            {LANGUAGES.map((l) => <option key={l.id} value={l.id}>{l.name}</option>)}
          </select>
          <button
            className={`fav-toggle ${form.is_favorite ? "active" : ""}`}
            onClick={() => onChange({ is_favorite: !form.is_favorite })}
            title={form.is_favorite ? t("snippet.unfavorite") : t("snippet.favorite")}
          >
            <svg width="16" height="16" viewBox="0 0 24 24" fill={form.is_favorite ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2">
              <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2" />
            </svg>
          </button>
        </div>
      </div>

      <input
        className="desc-input"
        placeholder={t("snippet.desc")}
        value={form.description}
        onChange={(e) => onChange({ description: e.target.value })}
      />

      <div className="tags-row">
        <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" />
          <line x1="7" y1="7" x2="7.01" y2="7" />
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
              placeholder={language === "zh" ? "输入后按回车" : "Press Enter"}
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
                  <button key={tag} type="button" className="tag-suggestion" onMouseDown={(e) => { e.preventDefault(); addTag(tag); }}>
                    {tag}
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      <div ref={splitRef} className="cm-editor-split">
        <div className="cm-main-pane">
          <CodeMirror
            value={form.content}
            className="snippet-codemirror"
            style={{ height: "100%" }}
            height="100%"
            extensions={mainExtensions}
            onChange={(val) => onChange({ content: val })}
            onCreateEditor={(view) => {
              cmRef.current = view;
              setMainScrollEl(view.scrollDOM as HTMLElement);
            }}
            basicSetup={{
              lineNumbers: true,
              drawSelection: true,
              highlightActiveLine: true,
              highlightSelectionMatches: true,
              autocompletion: true,
              bracketMatching: true,
              closeBrackets: true,
              foldGutter: true,
              indentOnInput: true,
            }}
          />
        </div>

        <div className={`codeglance-divider ${isDragging ? "active" : ""}`} onPointerDown={handleDividerPointerDown} />

        <MiniMap
          content={form.content}
          isDark={isDark}
          width={minimapWidth}
          mainScrollEl={mainScrollEl}
          scrollMainTo={scrollMainTo}
        />
      </div>

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
