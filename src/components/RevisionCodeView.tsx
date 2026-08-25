import { useEffect, useMemo, type ReactNode, type RefObject, type UIEvent } from "react";
import { useTranslation } from "react-i18next";
import { getCodeHighlightStyle, mountCodeHighlightStyle } from "./codeHighlightTheme";
import type { DiffRow, DiffRowKind, DiffSourceLine } from "./lineDiff";
import { sourceLines } from "./lineDiff";
import { getSyntaxHighlightRanges } from "./syntaxHighlight";

interface SourceLine extends DiffSourceLine {
  from: number;
  to: number;
}

interface RevisionCodeViewProps {
  content: string;
  language: string;
  theme: "dark" | "light";
  ariaLabel: string;
  rows?: DiffRow[];
  side?: "left" | "right";
  scrollRef?: RefObject<HTMLDivElement | null>;
  onScroll?: (event: UIEvent<HTMLDivElement>) => void;
}

function indexedSourceLines(content: string): SourceLine[] {
  const rawLines = content.split("\n");
  let offset = 0;
  return rawLines.map((raw, index) => {
    const text = raw.endsWith("\r") ? raw.slice(0, -1) : raw;
    const line = { lineNumber: index + 1, text, from: offset, to: offset + text.length };
    offset += raw.length + 1;
    return line;
  });
}

function renderHighlightedLine(
  line: SourceLine,
  ranges: ReturnType<typeof getSyntaxHighlightRanges>,
) {
  const parts: ReactNode[] = [];
  let cursor = line.from;
  for (const range of ranges) {
    if (range.to <= line.from || range.from >= line.to) continue;
    const from = Math.max(range.from, line.from);
    const to = Math.min(range.to, line.to);
    if (cursor < from) {
      parts.push(line.text.slice(cursor - line.from, from - line.from));
    }
    if (from < to) {
      parts.push(
        <span key={`${from}-${to}-${range.className}`} className={range.className}>
          {line.text.slice(from - line.from, to - line.from)}
        </span>,
      );
    }
    cursor = Math.max(cursor, to);
  }
  if (cursor < line.to) {
    parts.push(line.text.slice(cursor - line.from));
  }
  return parts.length ? parts : line.text;
}

function markerForKind(kind: DiffRowKind) {
  if (kind === "replace") return "~";
  if (kind === "delete") return "−";
  if (kind === "insert") return "+";
  return "";
}

/** Read-only, non-wrapping source renderer for revision preview and diff panes. */
export function RevisionCodeView({
  content,
  language,
  theme,
  ariaLabel,
  rows,
  side = "left",
  scrollRef,
  onScroll,
}: RevisionCodeViewProps) {
  const { t } = useTranslation();

  useEffect(() => {
    mountCodeHighlightStyle(theme);
  }, [theme]);

  const source = useMemo(() => indexedSourceLines(content), [content]);
  const sourceByLine = useMemo(
    () => new Map(source.map((line) => [line.lineNumber, line])),
    [source],
  );
  const ranges = useMemo(
    () => getSyntaxHighlightRanges(content, language, getCodeHighlightStyle(theme)),
    [content, language, theme],
  );
  const displayRows = useMemo(() => {
    if (!rows) {
      return sourceLines(content).map((line) => ({ kind: "equal" as const, line }));
    }
    return rows.map((row) => ({ kind: row.kind, line: row[side] }));
  }, [content, rows, side]);

  return (
    <div
      ref={scrollRef}
      className="revision-code-scroll"
      tabIndex={0}
      role="region"
      aria-label={ariaLabel}
      onScroll={onScroll}
    >
      <div className="revision-code-table" role="presentation">
        {displayRows.map((row, index) => {
          const line = row.line ? sourceByLine.get(row.line.lineNumber) : undefined;
          const rowKind = line ? row.kind : "placeholder";
          const modificationLabel = side === "left"
            ? "snippet.compareModifiedBeforeLine"
            : "snippet.compareModifiedAfterLine";
          return (
            <div
              key={`${row.line?.lineNumber ?? "blank"}-${index}`}
              className={`revision-code-row revision-code-row-${rowKind}`}
              aria-label={line ? undefined : ""}
            >
              <span className="revision-code-change" aria-hidden="true">
                {rowKind === "placeholder" ? "" : markerForKind(row.kind)}
              </span>
              <span className="revision-code-line-number" aria-hidden="true">
                {line?.lineNumber ?? ""}
              </span>
              <code className="revision-code-line">
                {line && row.kind === "replace" && (
                  <span className="sr-only">{t(modificationLabel)} </span>
                )}
                {line && row.kind === "delete" && (
                  <span className="sr-only">{t("snippet.compareRemovedLine")} </span>
                )}
                {line && row.kind === "insert" && (
                  <span className="sr-only">{t("snippet.compareAddedLine")} </span>
                )}
                {line ? renderHighlightedLine(line, ranges) : <span aria-hidden="true"> </span>}
              </code>
            </div>
          );
        })}
      </div>
    </div>
  );
}
