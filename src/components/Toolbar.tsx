import React, { useRef } from "react";
import { useTranslation } from "react-i18next";
import { Plus, Upload, Download, Sun, Moon } from "lucide-react";
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
  onOpenSettings,
  onSync,
  syncing,
  favoriteFilter,
  totalCount,
}: ToolbarProps) {
  const { t } = useTranslation();
  const importRef = useRef<HTMLInputElement>(null);

  return (
    <div className="toolbar">
      <div className="toolbar-brand">
        <svg width="22" height="22" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
          <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
          <polyline points="14 2 14 8 20 8"/>
          <line x1="16" y1="13" x2="8" y2="13"/>
          <line x1="16" y1="17" x2="8" y2="17"/>
          <line x1="10" y1="9" x2="8" y2="9"/>
        </svg>
        <span>{t("app.title")}</span>
        <span className="count-badge">{t("app.count", { count: totalCount })}</span>
      </div>

      <div className="toolbar-center">
        <div className="search-wrap">
          <svg className="search-icon" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/>
          </svg>
          <input
            className="search-input"
            placeholder={t("search.placeholder")}
            value={searchQuery}
            onChange={(e) => onSearchChange(e.target.value)}
            autoFocus
          />
          {searchQuery && (
            <button className="search-clear" onClick={() => onSearchChange("")}>
              ×
            </button>
          )}
        </div>

        <select
          className="lang-select"
          value={selectedLang}
          onChange={(e) => onLangChange(e.target.value)}
        >
          <option value="">{t("filter.all")}</option>
          {LANGUAGES.map((l) => (
            <option key={l.id} value={l.id}>
              {l.name}
            </option>
          ))}
        </select>

        <button
          className={`filter-btn ${favoriteFilter === true ? "active" : ""}`}
          onClick={() => onFavoriteFilter(favoriteFilter === true ? null : true)}
          title={t("filter.favorites")}
        >
          <svg width="16" height="16" viewBox="0 0 24 24" fill={favoriteFilter === true ? "currentColor" : "none"} stroke="currentColor" strokeWidth="2">
            <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
          </svg>
        </button>
      </div>

      <div className="toolbar-actions">
        <button className="action-btn" onClick={onExport} title={t("toolbar.export")}>
          <Download size={16} />
        </button>
        <button className="action-btn" onClick={() => importRef.current?.click()} title={t("toolbar.import")}>
          <Upload size={16} />
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
          className={`action-btn ${syncing ? "syncing" : ""}`}
          onClick={onSync ?? (() => {})}
          title={t("toolbar.sync")}
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
        <button className="action-btn" onClick={onOpenSettings} title={t("toolbar.settings")}>
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
            <circle cx="12" cy="12" r="3"/>
            <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06A1.65 1.65 0 0 0 4.68 15a1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.68a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06A1.65 1.65 0 0 0 19.4 9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
          </svg>
        </button>
        <button className="action-btn" onClick={onThemeToggle} title={t("toolbar.toggleTheme")}>
          {theme === "dark" ? <Sun size={16} /> : <Moon size={16} />}
        </button>
        <button className="new-btn" onClick={onNew} title={`${t("snippet.new")} (Ctrl+N)`}>
          <Plus size={16} />
          <span>{t("snippet.new")}</span>
        </button>
      </div>
    </div>
  );
}
