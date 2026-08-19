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

export type SnippetSort = "updated" | "recent";

export type QuickCaptureSource = "global_shortcut" | "tray";

/** A de-identified native quick-capture outcome; clipboard contents are never returned. */
export interface QuickCaptureCompletion {
  source: QuickCaptureSource;
  success: boolean;
  snippet_id?: string;
}

export interface SnippetQuery {
  query: string;
  language: string | null;
  favorite: boolean | null;
  exact_tag: string | null;
  sort?: SnippetSort;
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

export interface RevisionSummary {
  revision_id: string;
  parent_revision_id: string | null;
  revision_time: string;
  origin: "local" | "import" | "remote" | "conflict";
  deleted: boolean;
  conflict_of: string | null;
  is_current_head: boolean;
}

export interface RevisionContent {
  revision: RevisionSummary;
  snippet: Snippet | null;
  deleted_at: string | null;
}

export interface RevisionComparison {
  left: RevisionContent;
  right: RevisionContent;
}

export interface RevisionPage {
  items: RevisionSummary[];
  next_cursor: string | null;
}

export interface LocalSnapshot {
  id: string;
  created_at: string;
  schema_version: number;
  byte_count: number;
  snippet_count: number;
  verified_at: string;
  unavailable_at: string | null;
}

export interface SnapshotStatus {
  snapshots: LocalSnapshot[];
  latest_created_at: string | null;
  automatic_enabled: boolean;
  frequency: "daily" | "weekly";
  retention: 7 | 30 | 90;
}

export interface RestoreResult {
  restored_snapshot_id: string;
  emergency_snapshot_id: string;
}

export interface BulkMutationResult {
  requested_count: number;
  changed_count: number;
  unchanged_count: number;
}
