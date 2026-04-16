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

// ─── Glance (minimap) extensions ────────────────────────────────────────────

function buildGlanceExtensions(isDark: boolean, lang: string) {
  const bg = isDark ? "#0d1117" : "#ffffff";
  const fg = isDark ? "#8b949e" : "#6e7781";
  const cursor = isDark ? "#38bdf8" : "#0284c7";
  const selBg = isDark ? "rgba(56,189,248,0.35)" : "rgba(2,132,199,0.3)";

  const glanceLayout = EditorView.theme({
    "&": {
      position: "absolute", top: "0", bottom: "0", left: "0", right: "0",
      background: `${bg} !important`,
      fontSize: "13.5px",
    },
    ".cm-scroller": {
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
      overflowY: "auto !important",
      overflowX: "auto !important",
      height: "100%",
      display: "block",
    },
    ".cm-content": {
      fontFamily: "'JetBrains Mono', 'Fira Code', 'Cascadia Code', Consolas, monospace",
      caretColor: cursor,
    },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: cursor },
    ".cm-selectionBackground": { background: `${selBg} !important` },
    "&.cm-focused .cm-selectionBackground": { background: `${selBg} !important` },
    ".cm-gutters": {
      background: `${bg} !important`,
      borderRight: `1px solid ${isDark ? "#21262d" : "#d0d7de"}`,
      color: `${fg} !important`,
    },
    ".cm-activeLineGutter": { background: "transparent !important" },
    ".cm-activeLine": { background: "transparent !important" },
    // Hide fold gutter, line numbers too cluttering for minimap
    ".cm-gutterElement": { display: "none !important" },
    ".cm-lineNumbers .cm-gutterElement": { display: "none !important" },
    ".cm-foldGutter .cm-gutterElement": { display: "none !important" },
  }, { dark: isDark });

  return [
    EditorView.editable.of(false),
    EditorView.lineWrapping,
    getLangExtension(lang),
    isDark ? githubDark : githubLight,
    glanceLayout,
    isDark ? darkHighlight : lightHighlight,
  ];
}

// ─── Shadow DOM injection (main editor) ────────────────────────────────────

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
  const glanceRef = useRef<EditorView | null>(null);
  const mainScrollerRef = useRef<HTMLElement | null>(null);

  // ── State ─────────────────────────────────────────────────────────────────
  const [copied, setCopied] = useState(false);
  const [tagInput, setTagInput] = useState("");
  const [showTagSuggestions, setShowTagSuggestions] = useState(false);
  const [glanceWidth, setGlanceWidth] = useState(150);
  const [isDragging, setIsDragging] = useState(false);

  // ── Derived ───────────────────────────────────────────────────────────────
  const isDark = theme === "dark";
  const mainExtensions = useMemo(
    () => buildMainExtensions(isDark, form.language),
    [isDark, form.language]
  );
  const glanceExtensions = useMemo(
    () => buildGlanceExtensions(isDark, form.language),
    [isDark, form.language]
  );

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
        mainScrollerRef.current = sc;
      }
    };
    apply();
    requestAnimationFrame(apply);
    const ro = new ResizeObserver(apply);
    ro.observe(wrap);
    return () => ro.disconnect();
  }, [theme]);

  // ── Glance height fixup ────────────────────────────────────────────────────
  useEffect(() => {
    const wrap = editorWrapRef.current;
    if (!wrap) return;
    const apply = () => {
      const view = glanceRef.current;
      if (!view) return;
      const px = wrap.clientHeight;
      if (px <= 0) return;
      (view.dom as HTMLElement).style.height = `${px}px`;
    };
    apply();
    requestAnimationFrame(apply);
  }, [glanceWidth]);

  // ── Scroll sync: main → glance ─────────────────────────────────────────────
  const syncGlanceScroll = useCallback(() => {
    const main = mainScrollerRef.current;
    const glance = glanceRef.current;
    if (!main || !glance) return;

    const mainScrollable = main.scrollHeight - main.clientHeight;
    const glanceScrollable = glance.scrollHeight - glance.clientHeight;

    if (mainScrollable <= 0 || glanceScrollable <= 0) return;

    const ratio = main.scrollTop / mainScrollable;
    glance.scrollTop = ratio * glanceScrollable;
  }, []);

  // ── Glance divider drag ───────────────────────────────────────────────────
  const handleDividerPointerDown = useCallback((e: React.PointerEvent) => {
    e.preventDefault();
    setIsDragging(true);
    const startX = e.clientX;
    const startWidth = glanceWidth;
    const wrap = editorWrapRef.current;
    if (!wrap) return;

    const onMove = (ev: PointerEvent) => {
      const delta = startX - ev.clientX;
      const newWidth = Math.min(420, Math.max(80, startWidth + delta));
      setGlanceWidth(newWidth);
    };
    const onUp = () => {
      setIsDragging(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };
    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }, [glanceWidth]);

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

      {/* Editor + CodeGlance split */}
      <div className="cm-editor-split">
        {/* Main editor */}
        <div className="cm-main-pane" ref={editorWrapRef}>
          <CodeMirror
            value={form.content}
            extensions={mainExtensions}
            onChange={(val) => onChange({ content: val })}
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
                sc.addEventListener("scroll", syncGlanceScroll, { passive: true });
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

        {/* CodeGlance (minimap) */}
        <div className="codeglance-pane" style={{ width: glanceWidth }}>
          <CodeMirror
            value={form.content}
            extensions={glanceExtensions}
            onCreateEditor={(view) => {
              glanceRef.current = view;
              const sr = (view.dom as HTMLElement).shadowRoot;
              if (!sr) return;
              let s = sr.querySelector("[data-snpt-glance]") as HTMLStyleElement | null;
              if (!s) { s = document.createElement("style"); s.setAttribute("data-snpt-glance", ""); sr.appendChild(s); }
              s.textContent = injectShadowStyles(isDark);
              const sc = sr.querySelector(".cm-scroller") as HTMLElement | null;
              if (sc) { sc.style.overflowY = "auto"; sc.style.overflowX = "hidden"; sc.style.height = "100%"; }
            }}
            basicSetup={false}
          />
        </div>
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
