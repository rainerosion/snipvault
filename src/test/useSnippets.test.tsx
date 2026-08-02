import { act, renderHook, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it } from "vitest";
import { useSnippets } from "../hooks/useSnippets";
import { getTauriMocks } from "./setup";

const snippet = {
  id: "snippet-1",
  title: "Original",
  content: "const value = 1;",
  language: "javascript",
  description: "",
  tags: [],
  is_favorite: false,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  revision_id: "revision-1",
};

const summary = {
  ...snippet,
  content_preview: snippet.content,
};

const page = {
  items: [summary],
  next_cursor: null,
  total: 1,
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

describe("useSnippets mutation rejection state", () => {
  const mocks = getTauriMocks();

  beforeEach(() => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "query_snippets") return Promise.resolve(page);
      return Promise.resolve(undefined);
    });
  });

  it("sends the selected base revision when updating", async () => {
    mocks.invoke.mockImplementation((command: string) => {
      if (command === "update_snippet") return Promise.resolve(snippet);
      return Promise.resolve(page);
    });
    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.update(snippet.id, snippet.revision_id, {
        title: snippet.title,
        content: snippet.content,
        language: snippet.language,
        description: snippet.description,
        tags: snippet.tags,
        is_favorite: snippet.is_favorite,
      });
    });
    expect(mocks.invoke).toHaveBeenCalledWith(
      "update_snippet",
      expect.objectContaining({
        id: snippet.id,
        baseRevisionId: snippet.revision_id,
      }),
    );
  });

  it("preserves loaded state when delete rejects", async () => {
    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.load();
    });

    mocks.invoke.mockImplementation((command: string) => {
      if (command === "delete_snippet") {
        return Promise.reject({
          code: "database",
          message: "safe",
          retryable: true,
        });
      }
      return Promise.resolve(page);
    });

    await expect(result.current.remove(snippet.id)).rejects.toMatchObject({
      code: "database",
    });
    expect(result.current.snippets).toEqual([summary]);
  });

  it("preserves visible favorite state when favorite rejects", async () => {
    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.load();
    });

    mocks.invoke.mockImplementation((command: string) => {
      if (command === "toggle_favorite") {
        return Promise.reject({
          code: "not_found",
          message: "safe",
          retryable: false,
        });
      }
      return Promise.resolve(page);
    });

    await expect(
      result.current.toggleFavorite(snippet.id),
    ).rejects.toMatchObject({
      code: "not_found",
    });
    expect(result.current.snippets[0].is_favorite).toBe(false);
  });

  it("ignores a slower stale first-page response", async () => {
    const slow = deferred<typeof page>();
    const fastSummary = { ...summary, id: "fast", title: "Fast result" };
    const fastPage = { items: [fastSummary], next_cursor: null, total: 1 };
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "get_snippet_tags") return Promise.resolve([]);
      if (command !== "query_snippets") return Promise.resolve(undefined);
      const request = (args as { request: { query: string } }).request;
      return request.query === "slow"
        ? slow.promise
        : Promise.resolve(fastPage);
    });

    const { result } = renderHook(() => useSnippets());
    let slowLoad!: Promise<unknown>;
    await act(async () => {
      slowLoad = result.current.load({ query: "slow" });
    });
    await act(async () => {
      await result.current.load({ query: "fast" });
    });
    expect(result.current.snippets).toEqual([fastSummary]);

    await act(async () => {
      slow.resolve(page);
      await slowLoad;
    });
    expect(result.current.snippets).toEqual([fastSummary]);
  });

  it("resets pagination for a new filter and suppresses stale load-more", async () => {
    const append = deferred<typeof page>();
    const firstPage = { items: [summary], next_cursor: "cursor-1", total: 2 };
    const filteredSummary = { ...summary, id: "filtered", language: "python" };
    const filteredPage = {
      items: [filteredSummary],
      next_cursor: null,
      total: 1,
    };
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "get_snippet_tags") return Promise.resolve([]);
      if (command !== "query_snippets") return Promise.resolve(undefined);
      const request = (
        args as { request: { language: string | null; cursor: string | null } }
      ).request;
      if (request.cursor === "cursor-1") return append.promise;
      return Promise.resolve(
        request.language === "python" ? filteredPage : firstPage,
      );
    });

    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.load();
    });
    let appendLoad!: Promise<unknown>;
    await act(async () => {
      appendLoad = result.current.loadMore();
    });
    await act(async () => {
      await result.current.load({ language: "python" });
    });
    expect(result.current.snippets).toEqual([filteredSummary]);
    expect(result.current.hasMore).toBe(false);

    await act(async () => {
      append.resolve({
        items: [{ ...summary, id: "stale-append" }],
        next_cursor: null,
        total: 2,
      });
      await appendLoad;
    });
    expect(result.current.snippets).toEqual([filteredSummary]);
    await waitFor(() => expect(result.current.loadingMore).toBe(false));
  });

  it("appends a successful page once and advances pagination", async () => {
    const duplicate = { ...summary };
    const appended = { ...summary, id: "snippet-2", title: "Second page" };
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "get_snippet_tags") return Promise.resolve([]);
      if (command !== "query_snippets") return Promise.resolve(undefined);
      const cursor = (args as { request: { cursor: string | null } }).request
        .cursor;
      return Promise.resolve(
        cursor
          ? { items: [duplicate, appended], next_cursor: null, total: 2 }
          : { ...page, next_cursor: "next", total: 2 },
      );
    });

    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.load();
    });
    await act(async () => {
      await result.current.loadMore();
    });

    expect(result.current.snippets).toEqual([summary, appended]);
    expect(result.current.total).toBe(2);
    expect(result.current.hasMore).toBe(false);
    expect(result.current.loadMoreError).toBeNull();
  });

  it("allows only one in-flight append for the active cursor", async () => {
    const append = deferred<typeof page>();
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "get_snippet_tags") return Promise.resolve([]);
      if (command !== "query_snippets") return Promise.resolve(undefined);
      const cursor = (args as { request: { cursor: string | null } }).request
        .cursor;
      return cursor
        ? append.promise
        : Promise.resolve({ ...page, next_cursor: "next", total: 2 });
    });

    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.load();
    });
    let firstAppend!: Promise<unknown>;
    let duplicateAppend!: Promise<unknown>;
    await act(async () => {
      firstAppend = result.current.loadMore();
      duplicateAppend = result.current.loadMore();
    });
    await expect(duplicateAppend).resolves.toBeNull();
    expect(
      mocks.invoke.mock.calls.filter(
        ([command, args]) =>
          command === "query_snippets" &&
          (args as { request: { cursor: string | null } }).request.cursor ===
            "next",
      ),
    ).toHaveLength(1);

    await act(async () => {
      append.resolve({
        items: [{ ...summary, id: "snippet-2" }],
        next_cursor: null,
        total: 2,
      });
      await firstAppend;
    });
    expect(result.current.hasMore).toBe(false);
  });

  it("retains loaded cards when load-more fails", async () => {
    mocks.invoke.mockImplementation((command: string, args?: unknown) => {
      if (command === "get_snippet_tags") return Promise.resolve([]);
      if (command !== "query_snippets") return Promise.resolve(undefined);
      const cursor = (args as { request: { cursor: string | null } }).request
        .cursor;
      if (cursor) {
        return Promise.reject({
          code: "database",
          message: "safe",
          retryable: true,
        });
      }
      return Promise.resolve({ ...page, next_cursor: "next", total: 2 });
    });

    const { result } = renderHook(() => useSnippets());
    await act(async () => {
      await result.current.load();
    });
    await act(async () => {
      await expect(result.current.loadMore()).rejects.toMatchObject({
        code: "database",
      });
    });
    expect(result.current.snippets).toEqual([summary]);
    expect(result.current.loadMoreError).toMatchObject({ code: "database" });
  });
});
