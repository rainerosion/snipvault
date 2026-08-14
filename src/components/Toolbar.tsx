import React, { useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { useTranslation } from "react-i18next";
import { Plus, Upload, Download, Sun, Moon, Command, ArrowDownWideNarrow, History } from "lucide-react";
import { type SnippetSort } from "../types";
import { LANGUAGES } from "../utils/languages";

interface ToolbarProps {
  searchQuery: string;
  onSearchChange: (q: string) => void;
  selectedLang: string;
  onLangChange: (l: string) => void;
  onNew: () => void;
  onExport: () => void;
  onImportData: (jsonData: string) => void;
  onImportError: (msg: string) => void;
  theme: "dark" | "light";
  onThemeToggle: () => void;
  onFavoriteFilter: (fav: boolean | null) => void;
  sort?: SnippetSort;
  onSortChange?: (sort: SnippetSort) => void;
  onOpenCommandPalette?: () => void;
  searchInputRef?: React.RefObject<HTMLInputElement | null>;
  onOpenSettings: () => void;
  onSync?: () => void;
  syncing?: boolean;
  favoriteFilter: boolean | null;
  totalCount: number;
}

export function Toolbar({
  searchQuery,
  onSearchChange,
  selectedLang,
  onLangChange,
  onNew,
  onExport,
  onImportData,
  onImportError,
  theme,
  onThemeToggle,
  onFavoriteFilter,
  sort = "updated",
  onSortChange,
  onOpenCommandPalette,
  searchInputRef,
  onOpenSettings,
  onSync,
  syncing,
  favoriteFilter,
  totalCount,
}: ToolbarProps) {
  const { t } = useTranslation();
  const importRef = useRef<HTMLInputElement>(null);
  const sortButtonRef = useRef<HTMLButtonElement>(null);
  const sortTooltipRef = useRef<HTMLDivElement>(null);
  const [sortHovered, setSortHovered] = useState(false);
  const [sortFocused, setSortFocused] = useState(false);
  const [sortTooltipPosition, setSortTooltipPosition] = useState<{
    top: number;
    left: number;
    placement: "top" | "bottom";
    ready: boolean;
  } | null>(null);
  const sortTooltipVisible = sortHovered || sortFocused;
  const nextSort = sort === "updated" ? "recent" : "updated";
  const currentSortLabel = t(sort === "updated" ? "filter.sortUpdated" : "filter.sortRecent");
  const nextSortLabel = t(nextSort === "updated" ? "filter.sortUpdated" : "filter.sortRecent");
  const sortDescription = t("filter.sortToggle", {
    current: currentSortLabel,
    next: nextSortLabel,
  });
  const sortTooltipCurrent = t("filter.sortTooltipCurrent", { current: currentSortLabel });
  const sortTooltipHint = t("filter.sortTooltipHint");

  useLayoutEffect(() => {
    if (!sortTooltipVisible) {
      setSortTooltipPosition(null);
      return;
    }

    const updatePosition = () => {
      const button = sortButtonRef.current;
      const tooltip = sortTooltipRef.current;
      if (!button || !tooltip) return;

      const buttonRect = button.getBoundingClientRect();
      const tooltipRect = tooltip.getBoundingClientRect();
      const viewportInset = 8;
      const gap = 8;
      const fitsBelow = buttonRect.bottom + gap + tooltipRect.height <= window.innerHeight - viewportInset;
      const placement = fitsBelow ? "bottom" : "top";
      const top = placement === "bottom"
        ? buttonRect.bottom + gap
        : Math.max(viewportInset, buttonRect.top - gap - tooltipRect.height);
      const left = Math.min(
        Math.max(viewportInset, buttonRect.left + buttonRect.width / 2 - tooltipRect.width / 2),
        window.innerWidth - tooltipRect.width - viewportInset,
      );

      setSortTooltipPosition({ top, left, placement, ready: true });
    };

    updatePosition();
    window.addEventListener("resize", updatePosition);
    window.addEventListener("scroll", updatePosition, true);
    return () => {
      window.removeEventListener("resize", updatePosition);
      window.removeEventListener("scroll", updatePosition, true);
    };
  }, [sortTooltipCurrent, sortTooltipHint, sortTooltipVisible]);

  return (
    <div className="toolbar">
      <div className="toolbar-brand">
        <span className="toolbar-brand-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24" fill="none">
            <rect className="brand-icon-back" x="5" y="4" width="10" height="13" rx="3" />
            <rect className="brand-icon-front" x="9" y="7" width="10" height="13" rx="3" />
            <path className="brand-icon-line" d="M12.5 11.5h3" />
            <path className="brand-icon-line" d="M12.5 15h2" />
          </svg>
        </span>
        <span className="toolbar-brand-copy">
          <span className="toolbar-brand-title">{t("app.title")}</span>
          <span className="toolbar-brand-meta">
            <span>{t("app.subtitle")}</span>
            <span className="count-badge">{t("app.count", { count: totalCount })}</span>
          </span>
        </span>
      </div>

      <div className="toolbar-center">
        <div className="search-wrap">
          <svg className="search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input
            ref={searchInputRef}
            className="search-input"
            placeholder={t("search.placeholder")}
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            autoFocus
          />
          {searchQuery && (
            <button
              type="button"
              className="search-clear"
              onClick={() => onSearchChange("")}
              aria-label={t("search.clear")}
              title={t("search.clear")}
            >
              ×
            </button>
          )}
        </div>

        <select
          className="lang-select"
          value={selectedLang}
          onChange={(e) => onLangChange(e.target.value)}
          aria-label={t("filter.language")}
        >
          <option value="">{t("filter.all")}</option>
          {LANGUAGES.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>
        <button
          type="button"
          className={`filter-btn ${favoriteFilter === true ? "active" : ""}`}
          onClick={() => onFavoriteFilter(favoriteFilter === true ? null : true)}
          title={t("filter.favorites")}
          aria-label={t("filter.favorites")}
          aria-pressed={favoriteFilter === true}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill={favoriteFilter === true ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2">
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
          </svg>
        </button>
        <button
          ref={sortButtonRef}
          type="button"
          className={`filter-btn sort-mode-btn ${sort === "recent" ? "active" : ""}`}
          onClick={() => onSortChange?.(nextSort)}
          onPointerEnter={() => setSortHovered(true)}
          onPointerLeave={() => setSortHovered(false)}
          onFocus={() => setSortFocused(true)}
          onBlur={() => setSortFocused(false)}
          title={sortTooltipVisible ? undefined : sortDescription}
          aria-label={sortDescription}
        >
          {sort === "updated" ? <ArrowDownWideNarrow aria-hidden="true" size={16} /> : <History aria-hidden="true" size={16} />}
        </button>
      </div>

      {sortTooltipVisible && typeof document !== "undefined" && createPortal(
        <div
          ref={sortTooltipRef}
          className="sort-tooltip"
          data-placement={sortTooltipPosition?.placement ?? "bottom"}
          aria-hidden="true"
          style={{
            top: sortTooltipPosition?.top ?? 0,
            left: sortTooltipPosition?.left ?? 0,
            visibility: sortTooltipPosition?.ready ? "visible" : "hidden",
          }}
        >
          <span className="sort-tooltip-current">{sortTooltipCurrent}</span>
          <span className="sort-tooltip-hint">{sortTooltipHint}</span>
        </div>,
        document.body,
      )}
      <div className="toolbar-actions">
        <button
          type="button"
          className="action-btn"
          onClick={onOpenCommandPalette}
          title={t("commandPalette.open")}
          aria-label={t("commandPalette.open")}
        >
          <Command aria-hidden="true" size={16} />
        </button>
        <button type="button" className="action-btn" onClick={onExport} title={t("toolbar.export")} aria-label={t("toolbar.export")}>
          <Download aria-hidden="true" size={16} />
        </button>
        <button type="button" className="action-btn" onClick={() => importRef.current?.click()} title={t("toolbar.import")} aria-label={t("toolbar.import")}>
          <Upload aria-hidden="true" size={16} />
        </button>
        <input
          ref={importRef}
          type="file"
          accept=".json"
          style={{ display: "none" }}
          onChange={async (e) => {
            const file = e.target.files?.[0];
            if (!file) return;
            try {
              const text = await file.text();
              JSON.parse(text);
              onImportData(text);
            } catch {
              onImportError(t("errors.importInvalid"));
            }
            e.target.value = "";
          }}
        />
        <button
          type="button"
          className={`action-btn ${syncing ? "syncing" : ""}`}
          onClick={onSync ?? (() => {})}
          title={t("toolbar.sync")}
          aria-label={syncing ? t("settings.syncInProgress") : t("toolbar.sync")}
          aria-busy={syncing}
          disabled={syncing}
        >
          <svg
            width="16" height="16" viewBox="0 0 24 24" fill="none"
            stroke="currentColor" strokeWidth="2"
            style={{ color: "var(--accent)" }}
          >
            <path d="M17.5 19H9a7 7 0 1 1 6.71-9h1.79a4.5 4.5 0 1 1 0 9Z"/>
            {syncing && (
              <>
                <path className="sync-arrow" d="M21 12a9 9 0 1 1-9-9"
                  strokeDasharray="28" strokeDashoffset="0" strokeLinecap="round"/>
                <polyline points="21 3 21 9 15 9"/>
              </>
            )}
            {!syncing && (
              <path d="M21 12a9 9 0 1 1-9-9" opacity="0.4"/>
            )}
          </svg>
        </button>
        <button type="button" className="action-btn" onClick={onOpenSettings} title={t("toolbar.settings")} aria-label={t("toolbar.settings")}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        </button>
        <button type="button" className="action-btn" onClick={onThemeToggle} title={t("toolbar.toggleTheme")} aria-label={t("toolbar.toggleTheme")} aria-pressed={theme === "light"}>
          {theme === "dark" ? <Sun aria-hidden="true" size={16} /> : <Moon aria-hidden="true" size={16} />}
        </button>
        <button type="button" className="new-btn" onClick={onNew} title={`${t("snippet.new")} (Ctrl+N)`}>
          <Plus aria-hidden="true" size={16} />
          <span>{t("snippet.new")}</span>
        </button>
      </div>
    </div>
  );
}
