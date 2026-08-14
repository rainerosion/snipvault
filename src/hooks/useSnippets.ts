import { useCallback, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Snippet,
  SnippetForm,
  SnippetQuery,
  SnippetQueryResult,
  SnippetSort,
  SnippetSummary,
  type BulkMutationResult,
} from "../types";
import { normalizeCommandError, type CommandError } from "../utils/commandErrors";

export interface ExportResult {
  saved_in_downloads: boolean;
}

export interface ImportResult {
  input_count: number;
  inserted: number;
  updated: number;
  skipped: number;
}

export interface ListRequest {
  query?: string;
  language?: string | null;
  favorite?: boolean | null;
  exact_tag?: string | null;
  sort?: SnippetSort;
}

const PAGE_SIZE = 100;

function normalizeRequest(request: ListRequest = {}): ListRequest {
  return {
    query: request.query?.trim() ?? "",
    language: request.language || null,
    favorite: request.favorite ?? null,
    exact_tag: request.exact_tag || null,
    sort: request.sort ?? "updated",
  };
}

function requestKey(request: ListRequest): string {
  return JSON.stringify(normalizeRequest(request));
}

export function useSnippets() {
  const [snippets, setSnippets] = useState<SnippetSummary[]>([]);
  const [total, setTotal] = useState(0);
  const [nextCursor, setNextCursor] = useState<string | null>(null);
  const [tags, setTags] = useState<string[]>([]);
  const [loading, setLoading] = useState(false);
  const [loadingMore, setLoadingMore] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const [loadMoreError, setLoadMoreError] = useState<CommandError | null>(null);
  const generationRef = useRef(0);
  const requestRef = useRef<ListRequest>(normalizeRequest());
  const requestKeyRef = useRef(requestKey(requestRef.current));
  const snippetsRef = useRef<SnippetSummary[]>([]);
  const nextCursorRef = useRef<string | null>(null);
  const loadMoreInFlightRef = useRef(false);
  const loadMoreRequestRef = useRef(0);

  const applyItems = useCallback((items: SnippetSummary[]) => {
    snippetsRef.current = items;
    setSnippets(items);
  }, []);

  const loadTags = useCallback(async (generation: number) => {
    try {
      const values = await invoke<string[]>("get_snippet_tags");
      if (generation === generationRef.current) setTags(values);
    } catch {
      // Tag metadata is supplemental. The authoritative list error remains the
      // primary failure state and suggestions retain their last valid values.
    }
  }, []);

  const load = useCallback(
    async (request: ListRequest = requestRef.current): Promise<SnippetQueryResult | null> => {
      const normalized = normalizeRequest(request);
      const key = requestKey(normalized);
      requestRef.current = normalized;
      requestKeyRef.current = key;
      const generation = ++generationRef.current;
      ++loadMoreRequestRef.current;
      loadMoreInFlightRef.current = false;
      setLoading(true);
      setLoadingMore(false);
      setError(null);
      setLoadMoreError(null);
      try {
        const query: SnippetQuery = {
          query: normalized.query ?? "",
          language: normalized.language ?? null,
          favorite: normalized.favorite ?? null,
          exact_tag: normalized.exact_tag ?? null,
          sort: normalized.sort ?? "updated",
          limit: PAGE_SIZE,
          cursor: null,
        };
        const result = await invoke<SnippetQueryResult>("query_snippets", { request: query });
        if (generation !== generationRef.current || key !== requestKeyRef.current) return null;
        applyItems(result.items);
        setTotal(result.total);
        nextCursorRef.current = result.next_cursor;
        setNextCursor(result.next_cursor);
        void loadTags(generation);
        return result;
      } catch (cause) {
        if (generation === generationRef.current && key === requestKeyRef.current) {
          setError(normalizeCommandError(cause));
        }
        throw cause;
      } finally {
        if (generation === generationRef.current && key === requestKeyRef.current) {
          setLoading(false);
        }
      }
    },
    [applyItems, loadTags],
  );

  const loadMore = useCallback(async (): Promise<SnippetQueryResult | null> => {
    const cursor = nextCursorRef.current;
    if (!cursor || loadMoreInFlightRef.current) return null;
    loadMoreInFlightRef.current = true;
    const appendRequest = ++loadMoreRequestRef.current;
    const generation = generationRef.current;
    const key = requestKeyRef.current;
    const expectedIds = new Set(snippetsRef.current.map((snippet) => snippet.id));
    setLoadingMore(true);
    setLoadMoreError(null);
    try {
      const active = requestRef.current;
      const query: SnippetQuery = {
        query: active.query ?? "",
        language: active.language ?? null,
        favorite: active.favorite ?? null,
        exact_tag: active.exact_tag ?? null,
        sort: active.sort ?? "updated",
        limit: PAGE_SIZE,
        cursor,
      };
      const result = await invoke<SnippetQueryResult>("query_snippets", { request: query });
      if (
        generation !== generationRef.current ||
        key !== requestKeyRef.current ||
        cursor !== nextCursorRef.current
      ) {
        return null;
      }
      const combined = [
        ...snippetsRef.current,
        ...result.items.filter((snippet) => !expectedIds.has(snippet.id)),
      ];
      applyItems(combined);
      setTotal(result.total);
      nextCursorRef.current = result.next_cursor;
      setNextCursor(result.next_cursor);
      return result;
    } catch (cause) {
      if (
        appendRequest === loadMoreRequestRef.current &&
        generation === generationRef.current &&
        key === requestKeyRef.current &&
        cursor === nextCursorRef.current
      ) {
        setLoadMoreError(normalizeCommandError(cause));
      }
      throw cause;
    } finally {
      if (appendRequest === loadMoreRequestRef.current) {
        loadMoreInFlightRef.current = false;
        if (generation === generationRef.current && key === requestKeyRef.current) {
          setLoadingMore(false);
        }
      }
    }
  }, [applyItems]);

  const get = useCallback(async (id: string) => {
    return invoke<Snippet>("get_snippet", { id });
  }, []);

  const create = useCallback(async (form: SnippetForm) => {
    const id = crypto.randomUUID();
    return invoke<Snippet>("create_snippet", {
      id,
      title: form.title,
      content: form.content,
      language: form.language,
      description: form.description,
      tags: form.tags,
      isFavorite: form.is_favorite,
    });
  }, []);

  const update = useCallback(async (id: string, baseRevisionId: string, form: SnippetForm) => {
    return invoke<Snippet>("update_snippet", {
      id,
      title: form.title,
      content: form.content,
      language: form.language,
      description: form.description,
      tags: form.tags,
      isFavorite: form.is_favorite,
      baseRevisionId,
    });
  }, []);

  const remove = useCallback(async (id: string) => {
    return invoke<{ revision_id: string; deleted: boolean }>("delete_snippet", { id });
  }, []);

  const toggleFavorite = useCallback(async (id: string) => {
    return invoke<Snippet>("toggle_favorite", { id });
  }, []);
  const recordUsage = useCallback(
    async (id: string) => invoke<void>("record_snippet_usage", { id }),
    [],
  );
  const setManyFavorite = useCallback(
    async (ids: string[], isFavorite: boolean) =>
      invoke<BulkMutationResult>("set_snippets_favorite", { ids, isFavorite }),
    [],
  );
  const removeMany = useCallback(
    async (ids: string[]) => invoke<BulkMutationResult>("delete_snippets", { ids }),
    [],
  );

  const exportAll = useCallback(async () => invoke<string>("export_snippets"), []);
  const exportAllToFile = useCallback(
    async () => invoke<ExportResult>("export_snippets_to_file"),
    [],
  );
  const importAll = useCallback(
    async (jsonData: string) => invoke<ImportResult>("import_snippets", { jsonData }),
    [],
  );

  return {
    snippets,
    total,
    hasMore: nextCursor !== null,
    tags,
    loading,
    loadingMore,
    error,
    loadMoreError,
    load,
    loadMore,
    get,
    create,
    update,
    remove,
    toggleFavorite,
    recordUsage,
    setManyFavorite,
    removeMany,
    exportAll,
    exportAllToFile,
    importAll,
  };
}
