import { SnippetSummary } from "../types";
import { SnippetList } from "./SnippetList";

interface SidebarProps {
  snippets: SnippetSummary[];
  selectedId: string | null;
  onSelect: (s: SnippetSummary) => void;
  onDelete: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  loading: boolean;
  loadingMore: boolean;
  hasMore: boolean;
  error: string | null;
  loadMoreError: string | null;
  onRetry: () => void;
  onLoadMore: () => void;
}

export function Sidebar({
  snippets,
  selectedId,
  onSelect,
  onDelete,
  onToggleFavorite,
  loading,
  loadingMore,
  hasMore,
  error,
  loadMoreError,
  onRetry,
  onLoadMore,
}: SidebarProps) {
  return (
    <div className="snippet-list-container">
      <SnippetList
        snippets={snippets}
        selectedId={selectedId}
        onSelect={onSelect}
        onDelete={onDelete}
        onToggleFavorite={onToggleFavorite}
        loading={loading}
        loadingMore={loadingMore}
        hasMore={hasMore}
        error={error}
        loadMoreError={loadMoreError}
        onRetry={onRetry}
        onLoadMore={onLoadMore}
      />
    </div>
  );
}
