export interface SnippetSummary {
  id: string;
  title: string;
  language: string;
  description: string;
  tags: string[];
  is_favorite: boolean;
  created_at: string;
  updated_at: string;
  revision_id: string;
  content_preview: string;
}

export interface SnippetQuery {
  query: string;
  language: string | null;
  favorite: boolean | null;
  exact_tag: string | null;
  limit: number;
  cursor: string | null;
}

export interface SnippetQueryResult {
  items: SnippetSummary[];
  next_cursor: string | null;
  total: number;
}

export interface Snippet {
  id: string;
  title: string;
  content: string;
  language: string;
  description: string;
  tags: string[];
  is_favorite: boolean;
  created_at: string;
  updated_at: string;
  revision_id: string;
}

export interface SnippetForm {
  title: string;
  content: string;
  language: string;
  description: string;
  tags: string[];
  is_favorite: boolean;
}
