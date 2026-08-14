import { useContext } from "react";
import { useTranslation } from "react-i18next";
import { SnippetSummary } from "../types";
import { getLang } from "../utils/languages";
import { LanguageContext } from "../context/LanguageContext";

const EMPTY_SELECTION = new Set<string>();
const NOOP = () => {};

interface SnippetListProps {
  snippets: SnippetSummary[];
  selectedId: string | null;
  onSelect: (s: SnippetSummary) => void;
  onDelete: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  selectedIds?: Set<string>;
  onToggleSelection?: (id: string) => void;
  onSelectLoaded?: () => void;
  onClearSelection?: () => void;
  onSetFavorite?: (isFavorite: boolean) => void;
  onBulkDelete?: () => void;
  bulkBusy?: boolean;
  loading: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  error: string | null;
  loadMoreError: string | null;
  onRetry: () => void;
  onLoadMore: () => void;
}

function timeAgo(date: Date, lang: string): string {
  const diffMs = Date.now() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHr = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHr / 24);
  if (diffMin < 1) return lang === "zh" ? "刚刚" : "now";
  if (diffMin < 60) return lang === "zh" ? `${diffMin}分钟前` : `${diffMin}m ago`;
  if (diffHr < 24) return lang === "zh" ? `${diffHr}小时前` : `${diffHr}h ago`;
  if (diffDay < 30) return lang === "zh" ? `${diffDay}天前` : `${diffDay}d ago`;
  return lang === "zh" ? `${Math.floor(diffDay / 30)}个月前` : `${Math.floor(diffDay / 30)}mo ago`;
}

export function SnippetList({
  snippets,
  selectedId,
  onSelect,
  onDelete,
  onToggleFavorite,
  selectedIds = EMPTY_SELECTION,
  onToggleSelection = NOOP,
  onSelectLoaded = NOOP,
  onClearSelection = NOOP,
  onSetFavorite = NOOP,
  onBulkDelete = NOOP,
  bulkBusy = false,
  loading,
  loadingMore,
  hasMore,
  error,
  loadMoreError,
  onRetry,
  onLoadMore,
}: SnippetListProps) {
  const { t } = useTranslation();
  const { language } = useContext(LanguageContext);

  if (loading) {
    return (
      <div className="snippet-list-loading" role="status" aria-live="polite" aria-busy="true">
        <div className="spinner" aria-hidden="true" />
        <span>{t("sidebar.loading", "加载中...")}</span>
      </div>
    );
  }

  if (error && snippets.length === 0) {
    return (
      <div className="snippet-list-error" role="alert">
        <p>{error}</p>
        <button type="button" className="snippet-retry-btn" onClick={onRetry}>
          {t("sidebar.retry")}
        </button>
      </div>
    );
  }

  if (snippets.length === 0) {
    return (
      <div className="snippet-list-empty">
        <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5">
          <path d="M14.5 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7.5L14.5 2z"/>
          <polyline points="14 2 14 8 20 8"/>
        </svg>
        <p>{t("sidebar.empty")}</p>
        <p className="hint">Ctrl+N {t("snippet.new")}</p>
      </div>
    );
  }

  return (
    <div className="snippet-list" aria-busy={loadingMore}>
      {error && (
        <div className="snippet-refresh-error" role="alert">
          <span>{error}</span>
          <button type="button" className="snippet-retry-btn" onClick={onRetry}>
            {t("sidebar.retry")}
          </button>
        </div>
      )}
      <div className="bulk-actions" aria-label={t("bulk.actions")}>
        <span className="bulk-count">
          {t("bulk.selected", "{{count}} selected", { count: selectedIds.size })}
        </span>
        <button type="button" onClick={onSelectLoaded} disabled={bulkBusy || snippets.length === 0}>
          {t("bulk.selectLoaded")}
        </button>
        <button type="button" onClick={onClearSelection} disabled={bulkBusy || selectedIds.size === 0}>
          {t("bulk.clear")}
        </button>
        {selectedIds.size > 0 && (
          <>
            <button type="button" onClick={() => onSetFavorite(true)} disabled={bulkBusy}>
              {t("bulk.favorite")}
            </button>
            <button type="button" onClick={() => onSetFavorite(false)} disabled={bulkBusy}>
              {t("bulk.unfavorite")}
            </button>
            <button type="button" className="bulk-delete" onClick={onBulkDelete} disabled={bulkBusy}>
              {t("bulk.delete")}
            </button>
          </>
        )}
      </div>
      <ul className="snippet-items" aria-label={t("sidebar.snippetList")}>
        {snippets.map((s) => {
        const lang = getLang(s.language);
        const isSelected = s.id === selectedId;
        const selectionLimitReached = selectedIds.size >= 200 && !selectedIds.has(s.id);
        return (
          <li
            key={s.id}
            className={`snippet-item ${isSelected ? "selected" : ""} ${selectedIds.has(s.id) ? "bulk-selected" : ""}`}
          >
            <label className="snippet-select-check">
              <input
                type="checkbox"
                checked={selectedIds.has(s.id)}
                onChange={() => onToggleSelection(s.id)}
                disabled={bulkBusy || selectionLimitReached}
                aria-label={t("bulk.selectSnippet", { title: s.title || t("snippet.untitled") })}
              />
            </label>
            <button
              type="button"
              className="snippet-select-btn"
              onClick={() => onSelect(s)}
              aria-pressed={isSelected}
              aria-label={t("snippet.select", {
                title: s.title || t("snippet.untitled", "无标题"),
              })}
            >
            <div className="snippet-item-header">
              <span
                className="lang-dot"
                style={{ background: lang.color }}
                title={lang.name}
                aria-hidden="true"
              />
              <span className="snippet-title">{s.title || t("snippet.untitled", "无标题")}</span>
            </div>
            {s.description && (
              <p className="snippet-desc">{s.description}</p>
            )}
            <div className="snippet-meta">
              <span className="lang-tag" style={{ color: lang.color, borderColor: lang.color }}>
                {lang.name}
              </span>
              <span className="time-tag">
                {timeAgo(new Date(s.updated_at), language)}
              </span>
            </div>
            {s.content_preview && (
              <pre className="snippet-preview">
                {s.content_preview.split("\n").slice(0, 3).join("\n")}
              </pre>
            )}
            </button>
            <div className="snippet-actions">
              <button
                type="button"
                className={`fav-btn ${s.is_favorite ? "fav" : ""}`}
                onClick={() => onToggleFavorite(s.id)}
                aria-pressed={s.is_favorite}
                aria-label={t(
                  s.is_favorite ? "snippet.unfavoriteTitle" : "snippet.favoriteTitle",
                  { title: s.title || t("snippet.untitled") },
                )}
                title={s.is_favorite ? t("snippet.unfavorite") : t("snippet.favorite")}
              >
                <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24"
                  fill={s.is_favorite ? "currentColor" : "none"}
                  stroke="currentColor" strokeWidth="2">
                  <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
                </svg>
              </button>
              <button
                type="button"
                className="del-btn"
                onClick={() => onDelete(s.id)}
                aria-label={t("snippet.deleteTitle", {
                  title: s.title || t("snippet.untitled"),
                })}
                title={t("snippet.delete")}
              >
                <svg aria-hidden="true" width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                  <polyline points="3 6 5 6 21 6"/>
                  <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                </svg>
              </button>
            </div>
          </li>
        );
        })}
      </ul>
      <div className="snippet-pagination">
        {loadMoreError && <span role="alert">{loadMoreError}</span>}
        {hasMore && (
          <button type="button" className="snippet-load-more" onClick={onLoadMore} disabled={loadingMore}>
            {loadingMore ? t("sidebar.loadingMore", "加载更多...") : t("sidebar.loadMore", "加载更多")}
          </button>
        )}
      </div>
    </div>
  );
}
