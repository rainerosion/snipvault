import { SnippetSummary } from "../types";
import { SnippetList } from "./SnippetList";

const EMPTY_SELECTION = new Set<string>();
const NOOP = () => {};

interface SidebarProps {
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

export function Sidebar({
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
}: SidebarProps) {
  return (
    <div className="snippet-list-container">
      <SnippetList
        snippets={snippets}
        selectedId={selectedId}
        onSelect={onSelect}
        onDelete={onDelete}
        onToggleFavorite={onToggleFavorite}
        selectedIds={selectedIds}
        onToggleSelection={onToggleSelection}
        onSelectLoaded={onSelectLoaded}
        onClearSelection={onClearSelection}
        onSetFavorite={onSetFavorite}
        onBulkDelete={onBulkDelete}
        bulkBusy={bulkBusy}
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
