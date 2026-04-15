import { Snippet } from "../types";
import { SnippetList } from "./SnippetList";

interface SidebarProps {
  snippets: Snippet[];
  selectedId: string | null;
  onSelect: (s: Snippet) => void;
  onDelete: (id: string) => void;
  onToggleFavorite: (id: string) => void;
  loading: boolean;
}

export function Sidebar({
  snippets,
  selectedId,
  onSelect,
  onDelete,
  onToggleFavorite,
  loading,
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
      />
    </div>
  );
}
