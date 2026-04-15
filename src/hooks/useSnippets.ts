import { useState, useCallback } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Snippet, SnippetForm } from "../types";

export function useSnippets() {
  const [snippets, setSnippets] = useState<Snippet[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await invoke<Snippet[]>("get_snippets");
      setSnippets(data);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  const search = useCallback(
    async (query: string, language?: string, tag?: string) => {
      setLoading(true);
      setError(null);
      try {
        const data = await invoke<Snippet[]>("search_snippets", {
          query,
          language: language || null,
          tag: tag || null,
        });
        setSnippets(data);
      } catch (e) {
        setError(String(e));
      } finally {
        setLoading(false);
      }
    },
    []
  );

  const create = useCallback(async (form: SnippetForm) => {
    const id = crypto.randomUUID();
    await invoke("create_snippet", {
      id,
      title: form.title,
      content: form.content,
      language: form.language,
      description: form.description,
      tags: form.tags,
      isFavorite: form.is_favorite,
    });
  }, []);

  const update = useCallback(
    async (id: string, form: SnippetForm, updatedAt: string) => {
      await invoke("update_snippet", {
        id,
        title: form.title,
        content: form.content,
        language: form.language,
        description: form.description,
        tags: form.tags,
        isFavorite: form.is_favorite,
        updatedAt,
      });
    },
    []
  );

  const remove = useCallback(async (id: string) => {
    await invoke("delete_snippet", { id });
  }, []);

  const toggleFavorite = useCallback(async (id: string) => {
    const fav = await invoke<boolean>("toggle_favorite", { id });
    setSnippets((prev) =>
      prev.map((s) =>
        s.id === id ? { ...s, is_favorite: fav, updated_at: new Date().toISOString() } : s
      )
    );
    return fav;
  }, []);

  const exportAll = useCallback(async () => {
    return invoke<string>("export_snippets");
  }, []);

  const importAll = useCallback(async (jsonData: string) => {
    const count = await invoke<number>("import_snippets", { jsonData });
    await load();
    return count;
  }, [load]);

  return {
    snippets,
    loading,
    error,
    load,
    search,
    create,
    update,
    remove,
    toggleFavorite,
    exportAll,
    importAll,
  };
}
