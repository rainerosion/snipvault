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
}

export interface SnippetForm {
  title: string;
  content: string;
  language: string;
  description: string;
  tags: string[];
  is_favorite: boolean;
}
