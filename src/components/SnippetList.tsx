import { useContext } from "react";
import { useTranslation } from "react-i18next";
import { Snippet } from "../types";
import { getLang } from "../utils/languages";
import { LanguageContext } from "../context/LanguageContext";

interface SnippetListProps {
  snippets: Snippet[];
  selectedId: string | null;
  onSelect: (s: Snippet) => void;
  onDelete: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  loading: boolean;
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
  loading,
}: SnippetListProps) {
  const { t } = useTranslation();
  const { language } = useContext(LanguageContext);

  if (loading) {
    return (
      <div className="snippet-list-loading">
        <div className="spinner" />
        <span>{t("sidebar.loading", "加载中...")}</span>
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
    <div className="snippet-list">
      {snippets.map((s) => {
        const lang = getLang(s.language);
        const isSelected = s.id === selectedId;
        return (
          <div
            key={s.id}
            className={`snippet-item ${isSelected ? "selected" : ""}`}
            onClick={() => onSelect(s)}
          >
            <div className="snippet-item-header">
              <span
                className="lang-dot"
                style={{ background: lang.color }}
                title={lang.name}
              />
              <span className="snippet-title">{s.title || t("snippet.untitled", "无标题")}</span>
              <div className="snippet-actions">
                <button
                  className={`fav-btn ${s.is_favorite ? "fav" : ""}`}
                  onClick={(e) => {
                    e.stopPropagation();
                    onToggleFavorite(s.id);
                  }}
                  title={s.is_favorite ? t("snippet.unfavorite") : t("snippet.favorite")}
                >
                  <svg width="13" height="13" viewBox="0 0 24 24"
                    fill={s.is_favorite ? "currentColor" : "none"}
                    stroke="currentColor" strokeWidth="2">
                    <polygon points="12 2 15.09 8.26 22 9.27 17 14.14 18.18 21.02 12 17.77 5.82 21.02 7 14.14 2 9.27 8.91 8.26 12 2"/>
                  </svg>
                </button>
                <button
                  className="del-btn"
                  onClick={(e) => {
                    e.stopPropagation();
                    onDelete(s.id);
                  }}
                  title={t("snippet.delete")}
                >
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
                    <polyline points="3 6 5 6 21 6"/>
                    <path d="M19 6v14a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V6m3 0V4a1 1 0 0 1 1-1h4a1 1 0 0 1 1 1v2"/>
                  </svg>
                </button>
              </div>
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
            {s.content && (
              <pre className="snippet-preview">
                {s.content.split("\n").slice(0, 3).join("\n")}
              </pre>
            )}
          </div>
        );
      })}
    </div>
  );
}
