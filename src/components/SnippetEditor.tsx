import { useCallback, useEffect, useMemo, useRef, useState, useContext } from "react";
import { useTranslation } from "react-i18next";
import CodeMirror from "@uiw/react-codemirror";
import { EditorView } from "@codemirror/view";
import { HighlightStyle, syntaxHighlighting } from "@codemirror/language";
import { tags as t } from "@lezer/highlight";
import { getSyntaxHighlightRanges, type SyntaxHighlightRange } from "./syntaxHighlight";
import { githubDark, githubDarkStyle, githubLight, githubLightStyle } from "@uiw/codemirror-theme-github";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { Snippet, SnippetForm } from "../types";
import { LANGUAGES, type LanguageId } from "../utils/languages";
import { LanguageContext } from "../context/LanguageContext";
import { getLanguageExtensions } from "./languageExtensions";

interface SnippetEditorProps {
  snippet: Snippet | null;
  isNew: boolean;
  form: SnippetForm;
  onChange: (f: Partial<SnippetForm>) => void;
  onSave: () => void;
  onCancel: () => void;
  onClipboardError?: (error: unknown) => void;
  theme: "dark" | "light";
  lineWrap: boolean;
  saving: boolean;
  isDirty: boolean;
  tagOptions: string[];
}

const DARK_SYNTAX_COLORS = {
  keyword: "#ff7b72",
  name: "#ffa657",
  functionName: "#d2a8ff",
  type: "#79c0ff",
  string: "#a5d6ff",
  number: "#79c0ff",
  comment: "#8b949e",
  punctuation: "#ff7b72",
  plain: "#c9d1d9",
};

const LIGHT_SYNTAX_COLORS = {
  keyword: "#cf222e",
  name: "#953800",
  functionName: "#8250df",
  type: "#0550ae",
  string: "#0a3069",
  number: "#0550ae",
  comment: "#6e7781",
  punctuation: "#cf222e",
  plain: "#24292f",
};

const darkHighlightStyle = HighlightStyle.define([
  ...githubDarkStyle,
  { tag: t.keyword, color: DARK_SYNTAX_COLORS.keyword },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: DARK_SYNTAX_COLORS.name },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: DARK_SYNTAX_COLORS.functionName },
  { tag: [t.labelName], color: "#7ee787" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: DARK_SYNTAX_COLORS.name },
  { tag: [t.definition(t.name), t.separator], color: DARK_SYNTAX_COLORS.plain },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: DARK_SYNTAX_COLORS.type },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: DARK_SYNTAX_COLORS.punctuation },
  { tag: [t.meta, t.comment], color: DARK_SYNTAX_COLORS.comment, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: DARK_SYNTAX_COLORS.string, textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: DARK_SYNTAX_COLORS.type },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: DARK_SYNTAX_COLORS.keyword },
  { tag: [t.processingInstruction, t.string, t.inserted], color: DARK_SYNTAX_COLORS.string },
  { tag: t.number, color: DARK_SYNTAX_COLORS.number },
  { tag: t.invalid, color: DARK_SYNTAX_COLORS.keyword },
]);

const lightHighlightStyle = HighlightStyle.define([
  ...githubLightStyle,
  { tag: t.keyword, color: LIGHT_SYNTAX_COLORS.keyword },
  { tag: [t.name, t.deleted, t.character, t.propertyName, t.macroName], color: LIGHT_SYNTAX_COLORS.name },
  { tag: [t.function(t.variableName), t.function(t.propertyName)], color: LIGHT_SYNTAX_COLORS.functionName },
  { tag: [t.labelName], color: "#116329" },
  { tag: [t.color, t.constant(t.name), t.standard(t.name)], color: LIGHT_SYNTAX_COLORS.name },
  { tag: [t.definition(t.name), t.separator], color: LIGHT_SYNTAX_COLORS.plain },
  { tag: [t.typeName, t.className, t.number, t.changed, t.annotation, t.modifier, t.self, t.namespace], color: LIGHT_SYNTAX_COLORS.type },
  { tag: [t.operator, t.operatorKeyword, t.url, t.escape, t.regexp, t.link, t.special(t.string)], color: LIGHT_SYNTAX_COLORS.punctuation },
  { tag: [t.meta, t.comment], color: LIGHT_SYNTAX_COLORS.comment, fontStyle: "italic" },
  { tag: t.strong, fontWeight: "bold" },
  { tag: t.emphasis, fontStyle: "italic" },
  { tag: t.strikethrough, textDecoration: "line-through" },
  { tag: t.link, color: LIGHT_SYNTAX_COLORS.string, textDecoration: "underline" },
  { tag: t.heading, fontWeight: "bold", color: LIGHT_SYNTAX_COLORS.type },
  { tag: [t.atom, t.bool, t.special(t.variableName)], color: LIGHT_SYNTAX_COLORS.keyword },
  { tag: [t.processingInstruction, t.string, t.inserted], color: LIGHT_SYNTAX_COLORS.string },
  { tag: t.number, color: LIGHT_SYNTAX_COLORS.number },
  { tag: t.invalid, color: LIGHT_SYNTAX_COLORS.keyword },
]);

const darkHighlight = syntaxHighlighting(darkHighlightStyle);
const lightHighlight = syntaxHighlighting(lightHighlightStyle);

function buildMainExtensions(isDark: boolean, lang: string, lineWrap: boolean) {
  const selBg = isDark ? "rgba(56,189,248,0.62)" : "rgba(2,132,199,0.42)";
  const selBgF = isDark ? "rgba(56,189,248,0.74)" : "rgba(2,132,199,0.56)";
  const cursor = isDark ? "#38bdf8" : "#0284c7";

  const cmLayout = EditorView.theme({
    "&": {
      height: "100%",
      minHeight: "0",
      fontSize: "13.5px",
    },
    ".cm-editor": {
      minWidth: "0",
    },
    ".cm-scroller": {
      height: "100%",
      minHeight: "0",
      minWidth: "0",
      width: "100%",
      fontFamily: "ui-monospace, 'Cascadia Code', 'SFMono-Regular', Consolas, monospace",
      overflowY: "auto !important",
      overflowX: lineWrap ? "hidden !important" : "auto !important",
    },
    ".cm-content": {
      fontFamily: "ui-monospace, 'Cascadia Code', 'SFMono-Regular', Consolas, monospace",
      caretColor: cursor,
      whiteSpace: lineWrap ? "pre-wrap" : "pre",
      width: lineWrap ? "auto" : "max-content",
      minWidth: lineWrap ? "0" : "100%",
      maxWidth: lineWrap ? "100%" : "none",
      wordBreak: lineWrap ? "break-word" : "normal",
      overflowWrap: lineWrap ? "anywhere" : "normal",
      boxSizing: "border-box",
    },
    ".cm-line": {
      whiteSpace: lineWrap ? "pre-wrap" : "pre",
      wordBreak: lineWrap ? "break-word" : "normal",
      overflowWrap: lineWrap ? "anywhere" : "normal",
      maxWidth: lineWrap ? "100%" : "none",
      boxSizing: "border-box",
    },
    ".cm-cursor, .cm-dropCursor": { borderLeftColor: cursor },
    "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": { background: selBg },
    "&.cm-focused .cm-selectionBackground": { background: selBgF },
  });

  return [
    ...(lineWrap ? [EditorView.lineWrapping] : []),
    isDark ? darkHighlight : lightHighlight,
    isDark ? githubDark : githubLight,
    getLanguageExtensions(
      LANGUAGES.some((language) => language.id === lang)
        ? (lang as LanguageId)
        : "plaintext",
    ),
    cmLayout,
    isDark ? darkHighlight : lightHighlight,
  ];
}

interface MiniMapProps {
  content: string;
  language: string;
  highlightStyle: HighlightStyle;
  isDark: boolean;
  width: number;
  mainScrollEl: HTMLElement | null;
  editorView: EditorView | null;
  scrollMainTo: (scrollTop: number) => void;
}

const GLANCE_LINE_H = 4;
const GLANCE_PADDING_X = 4;
const GLANCE_MAX_CHARS_FOR_SCALING = 220;
const GLANCE_MIN_BASELINE = 24;
const GLANCE_IQR_MULTIPLIER = 1.5;
const GLANCE_MIN_OUTLIER_GAP = 12;

function colorForHighlightClass(
  editorView: EditorView | null,
  className: string | null,
  fallbackColor: string,
  colorCache: Map<string, string>,
): string {
  if (!className) return fallbackColor;

  const cached = colorCache.get(className);
  if (cached) return cached;

  const parent = editorView?.contentDOM;
  if (!parent) return fallbackColor;

  const sample = document.createElement("span");
  sample.className = className;
  sample.textContent = "x";
  sample.style.cssText = "position:absolute;visibility:hidden;pointer-events:none;";
  parent.appendChild(sample);
  const color = getComputedStyle(sample).color || fallbackColor;
  sample.remove();
  colorCache.set(className, color);
  return color;
}

function getLineHighlightSegments(
  lineText: string,
  lineStart: number,
  highlightRanges: SyntaxHighlightRange[],
  resolveColor: (className: string | null) => string,
): { length: number; color: string; visible: boolean }[] {
  if (!lineText) return [];

  const lineEnd = lineStart + lineText.length;
  const segments: { length: number; color: string; visible: boolean }[] = [];
  let position = lineStart;

  const addSegment = (from: number, to: number, className: string | null) => {
    const length = to - from;
    if (length <= 0) return;

    const text = lineText.slice(from - lineStart, to - lineStart);
    const visible = /\S/.test(text);
    const color = resolveColor(className);
    const previous = segments[segments.length - 1];
    if (previous?.color === color && previous.visible === visible) {
      previous.length += length;
    } else {
      segments.push({ length, color, visible });
    }
  };

  for (const range of highlightRanges) {
    if (range.to <= position) continue;
    if (range.from >= lineEnd) break;

    const from = Math.max(position, range.from, lineStart);
    const to = Math.min(range.to, lineEnd);
    addSegment(position, from, null);
    addSegment(from, to, range.className);
    position = to;
  }

  addSegment(position, lineEnd, null);
  return segments;
}

function MiniMap({
  content,
  language,
  highlightStyle,
  isDark,
  width,
  mainScrollEl,
  editorView,
  scrollMainTo,
}: MiniMapProps) {
  const paneRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const viewportRef = useRef<HTMLDivElement>(null);
  const syncingFromMiniRef = useRef(false);
  const draggingViewportRef = useRef<{ pointerId: number; pointerOffsetInViewport: number } | null>(null);
  const suppressClickRef = useRef(false);

  const [isViewportDragging, setIsViewportDragging] = useState(false);
  const [paneClientHeight, setPaneClientHeight] = useState(0);
  const [mainCanScroll, setMainCanScroll] = useState(false);
  const [fallbackColor, setFallbackColor] = useState("");
  const colorCacheRef = useRef(new Map<string, string>());
  const highlightRanges = useMemo(
    () => getSyntaxHighlightRanges(content, language, highlightStyle),
    [content, language, highlightStyle],
  );
  useEffect(() => {
    colorCacheRef.current.clear();
    const contentElement = editorView?.contentDOM;
    setFallbackColor(contentElement ? getComputedStyle(contentElement).color : "");
  }, [editorView, isDark]);

  const resolveColor = useCallback(
    (className: string | null) =>
      colorForHighlightClass(
        editorView,
        className,
        fallbackColor || (isDark ? DARK_SYNTAX_COLORS.plain : LIGHT_SYNTAX_COLORS.plain),
        colorCacheRef.current,
      ),
    [editorView, fallbackColor, isDark],
  );

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

  const lineLens = useMemo(
    () => lines.map((l) => l.length).filter((len) => len > 0),
    [lines]
  );
  const maxLen = useMemo(() => Math.max(...lineLens, 1), [lineLens]);

  const effectiveMaxLen = useMemo(() => {
    if (lineLens.length === 0) return GLANCE_MIN_BASELINE;

    const sorted = [...lineLens].sort((a, b) => a - b);
    const q1 = sorted[Math.floor((sorted.length - 1) * 0.25)] ?? sorted[0];
    const q3 = sorted[Math.floor((sorted.length - 1) * 0.75)] ?? sorted[sorted.length - 1];
    const iqr = Math.max(0, q3 - q1);
    const outlierThreshold = q3 + Math.max(GLANCE_MIN_OUTLIER_GAP, iqr * GLANCE_IQR_MULTIPLIER);

    const normalLens = sorted.filter((len) => len <= outlierThreshold);
    const normalMax = normalLens.length > 0 ? normalLens[normalLens.length - 1] : sorted[sorted.length - 1];

    return Math.min(GLANCE_MAX_CHARS_FOR_SCALING, Math.max(GLANCE_MIN_BASELINE, normalMax));
  }, [lineLens]);

  const hasExtremeLine = maxLen > effectiveMaxLen;
  const availW = Math.max(1, width - GLANCE_PADDING_X * 2);
  const scaleX = availW / Math.max(1, effectiveMaxLen);

  const drawCanvas = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    canvas.width = width;
    canvas.height = totalH;
    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    ctx.clearRect(0, 0, width, totalH);

    let lineStart = 0;
    lines.forEach((lineText, i) => {
      const y = i * GLANCE_LINE_H;
      ctx.fillStyle = bg;
      ctx.fillRect(0, y, width, GLANCE_LINE_H);
      if (!lineText.trim()) {
        lineStart += lineText.length + 1;
        return;
      }

      const segments = getLineHighlightSegments(
        lineText,
        lineStart,
        highlightRanges,
        resolveColor,
      );
      let x = GLANCE_PADDING_X;

      for (const segment of segments) {
        const segmentWidth = Math.min(segment.length * scaleX, width - x - 1);
        if (segmentWidth > 0.5) {
          if (segment.visible) {
            ctx.fillStyle = segment.color;
            ctx.beginPath();
            ctx.roundRect(x, y + 0.5, segmentWidth, GLANCE_LINE_H - 1, 1);
            ctx.fill();
          }
          x += segmentWidth;
          if (x >= width - GLANCE_PADDING_X) break;
        }
      }

      if (hasExtremeLine && lineText.length > effectiveMaxLen && x < width - GLANCE_PADDING_X) {
        const remainW = width - GLANCE_PADDING_X - x;
        ctx.fillStyle = resolveColor(null);
        ctx.fillRect(x, y + 1, remainW, GLANCE_LINE_H - 2);
      }

      lineStart += lineText.length + 1;
    });
  }, [
    lines,
    width,
    totalH,
    bg,
    highlightRanges,
    resolveColor,
    scaleX,
    hasExtremeLine,
    effectiveMaxLen,
  ]);

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
    <div className="minimap-wrap" style={{ width }} aria-hidden="true">
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

function TagCombobox({
  tags,
  tagOptions,
  onChange,
}: {
  tags: string[];
  tagOptions: string[];
  onChange: (tags: string[]) => void;
}) {
  const { t } = useTranslation();
  const [tagInput, setTagInput] = useState("");
  const [showTagSuggestions, setShowTagSuggestions] = useState(false);
  const [activeTagIndex, setActiveTagIndex] = useState(0);
  const tagInputRef = useRef<HTMLInputElement>(null);
  const tagListboxId = "snippet-tag-options";

  const filteredTagOptions = useMemo(() => {
    const query = tagInput.trim().toLowerCase();
    return tagOptions
      .filter(
        (tag) =>
          !tags.includes(tag) &&
          (query === "" || tag.toLowerCase().includes(query)),
      )
      .slice(0, 8);
  }, [tagInput, tagOptions, tags]);

  const addTag = useCallback(
    (raw: string) => {
      const tag = raw.trim();
      if (!tag) return;
      if (!tags.includes(tag)) onChange([...tags, tag]);
      setTagInput("");
      setShowTagSuggestions(false);
      setActiveTagIndex(0);
    },
    [onChange, tags],
  );

  const removeTag = useCallback(
    (tag: string) => onChange(tags.filter((item) => item !== tag)),
    [onChange, tags],
  );

  useEffect(() => {
    setActiveTagIndex((current) =>
      filteredTagOptions.length === 0
        ? 0
        : Math.min(current, filteredTagOptions.length - 1),
    );
  }, [filteredTagOptions.length]);

  const handleTagKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLInputElement>) => {
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        if (filteredTagOptions.length === 0) return;
        event.preventDefault();
        setShowTagSuggestions(true);
        setActiveTagIndex((current) => {
          const direction = event.key === "ArrowDown" ? 1 : -1;
          return (
            (current + direction + filteredTagOptions.length) %
            filteredTagOptions.length
          );
        });
        return;
      }

      if (event.key === "Escape" && showTagSuggestions) {
        event.preventDefault();
        setShowTagSuggestions(false);
        return;
      }

      if (event.key === "Enter" || event.key === ",") {
        event.preventDefault();
        const activeSuggestion = showTagSuggestions
          ? filteredTagOptions[activeTagIndex]
          : undefined;
        addTag(activeSuggestion ?? tagInput);
        return;
      }

      if (event.key === "Backspace" && !tagInput && tags.length > 0) {
        removeTag(tags[tags.length - 1]);
      }
    },
    [
      activeTagIndex,
      addTag,
      filteredTagOptions,
      removeTag,
      showTagSuggestions,
      tagInput,
      tags,
    ],
  );

  return (
    <div className="editor-tags">
      {tags.map((tag) => (
        <span key={tag} className="tag-chip">
          <span>{tag}</span>
          <button
            type="button"
            className="tag-remove"
            onClick={() => removeTag(tag)}
            aria-label={t("snippet.removeTag", { tag })}
            title={t("snippet.removeTag", { tag })}
          >
            ×
          </button>
        </span>
      ))}
      <div className="tag-input-wrap">
        <input
          ref={tagInputRef}
          className="tag-input"
          placeholder={t("snippet.tags")}
          value={tagInput}
          role="combobox"
          aria-label={t("snippet.tags")}
          aria-expanded={showTagSuggestions && filteredTagOptions.length > 0}
          aria-controls={tagListboxId}
          aria-autocomplete="list"
          aria-activedescendant={
            showTagSuggestions && filteredTagOptions[activeTagIndex]
              ? `${tagListboxId}-${activeTagIndex}`
              : undefined
          }
          onClick={() => {
            setShowTagSuggestions(true);
            setActiveTagIndex(0);
          }}
          onFocus={() => {
            setShowTagSuggestions(true);
            setActiveTagIndex(0);
          }}
          onBlur={(event) => {
            const next = event.relatedTarget;
            if (
              !(next instanceof Node) ||
              !event.currentTarget.parentElement?.contains(next)
            ) {
              setShowTagSuggestions(false);
            }
          }}
          onChange={(event) => {
            setTagInput(event.target.value);
            setShowTagSuggestions(true);
            setActiveTagIndex(0);
          }}
          onKeyDown={handleTagKeyDown}
        />
        {showTagSuggestions && filteredTagOptions.length > 0 && (
          <div id={tagListboxId} className="tag-suggestions" role="listbox">
            {filteredTagOptions.map((tag, index) => (
              <button
                id={`${tagListboxId}-${index}`}
                key={tag}
                type="button"
                className={`tag-suggestion ${index === activeTagIndex ? "active" : ""}`}
                role="option"
                aria-selected={index === activeTagIndex}
                tabIndex={-1}
                onPointerDown={(event) => event.preventDefault()}
                onClick={() => {
                  addTag(tag);
                  tagInputRef.current?.focus();
                }}
                onMouseEnter={() => setActiveTagIndex(index)}
              >
                {tag}
              </button>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

export const SnippetTagCombobox = TagCombobox;

export function SnippetEditor({
  snippet, isNew, form, onChange, onSave, onCancel, onClipboardError,
  theme, lineWrap, saving, isDirty, tagOptions,
}: SnippetEditorProps) {
  const { t } = useTranslation();
  const { language } = useContext(LanguageContext);

  const cmRef = useRef<EditorView | null>(null);
  const [mainScrollEl, setMainScrollEl] = useState<HTMLElement | null>(null);
  const [editorView, setEditorView] = useState<EditorView | null>(null);

  const splitRef = useRef<HTMLDivElement | null>(null);
  const [copied, setCopied] = useState(false);
  const [minimapWidth, setMinimapWidth] = useState(96);
  const [isDragging, setIsDragging] = useState(false);

  const isDark = theme === "dark";
  const mainExtensions = useMemo(
    () => buildMainExtensions(isDark, form.language, lineWrap),
    [isDark, form.language, lineWrap]
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
      const dynamicMax = splitWidth > 0 ? Math.floor(splitWidth * 0.45) : 360;
      const maxWidth = Math.max(96, Math.min(360, dynamicMax));
      setMinimapWidth(Math.min(maxWidth, Math.max(96, next)));
    };
    const onUp = () => {
      setIsDragging(false);
      document.removeEventListener("pointermove", onMove);
      document.removeEventListener("pointerup", onUp);
    };

    document.addEventListener("pointermove", onMove);
    document.addEventListener("pointerup", onUp);
  }, [minimapWidth]);

  const handleCopy = useCallback(async () => {
    if (!form.content) return;
    try {
      await writeText(form.content);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (e) {
      onClipboardError?.(e);
    }
  }, [form.content, onClipboardError]);

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
            type="button"
            className={`fav-toggle ${form.is_favorite ? "active" : ""}`}
            onClick={() => onChange({ is_favorite: !form.is_favorite })}
            title={form.is_favorite ? t("snippet.unfavorite") : t("snippet.favorite")}
            aria-label={form.is_favorite ? t("snippet.unfavorite") : t("snippet.favorite")}
            aria-pressed={form.is_favorite}
          >
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill={form.is_favorite ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2">
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
        <TagCombobox
          tags={form.tags}
          tagOptions={tagOptions}
          onChange={(tags) => onChange({ tags })}
        />
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
              setEditorView(view);
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

        <div
          className={`codeglance-divider ${isDragging ? "active" : ""}`}
          onPointerDown={handleDividerPointerDown}
          aria-hidden="true"
          title={t("snippet.minimapResize")}
        />

        <MiniMap
          content={form.content}
          language={form.language}
          highlightStyle={isDark ? darkHighlightStyle : lightHighlightStyle}
          isDark={isDark}
          width={minimapWidth}
          mainScrollEl={mainScrollEl}
          editorView={editorView}
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
            <button type="button" className={`btn-copy ${copied ? "copied" : ""}`} onClick={handleCopy} title={t("snippet.copy")} aria-live="polite">
              {copied ? t("snippet.copied") : t("snippet.copy")}
            </button>
          )}
          {(isNew || isDirty) && (
            <button type="button" className="btn-cancel" onClick={onCancel}>
              {isNew ? t("snippet.cancel") : t("snippet.cancelEdit")}
            </button>
          )}
          <button type="button" className="btn-save" onClick={onSave} disabled={saving} aria-busy={saving}>
            {saving ? t("snippet.saveInProgress") : isNew ? t("snippet.create") : t("snippet.save")}
          </button>
        </div>
      </div>
    </div>
  );
}
