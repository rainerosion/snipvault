import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import App from "../App";
import { LanguageContext } from "../context/LanguageContext";
import i18n from "../i18n";
import {
  SettingsProvider,
  type SettingsApi,
  type SyncCompletionEvent,
} from "../hooks/useSettings";
import { ThemeContext } from "../main";
import { getTauriMocks } from "./setup";
import { DEFAULT_SETTINGS } from "./settingsFixtures";

vi.mock("../main", async () => {
  const { createContext } = await import("react");
  return {
    ThemeContext: createContext({
      theme: "dark" as const,
      accentPreset: "sky" as const,
      setTheme: () => undefined,
    }),
  };
});

vi.mock("../components/SnippetEditor", () => ({
  SnippetEditor: ({
    form,
    onChange,
  }: {
    form: { title: string };
    onChange: (change: { title: string }) => void;
  }) => (
    <div data-testid="snippet-editor">
      <span data-testid="editor-title">{form.title}</span>
      <button
        type="button"
        onClick={() => onChange({ title: "Dirty local title" })}
      >
        Make dirty
      </button>
    </div>
  ),
}));

const EMPTY_SYNC_RESULT = {
  success: true,
  message: "sync done",
  uploaded: false,
  uploaded_count: 0,
  downloaded_count: 0,
  deleted_count: 0,
  conflict_count: 0,
  pending_count: 0,
  protocol_version: 2,
  manifest_generation: 0,
  total_count: 0,
};

function makeApi(overrides: Partial<SettingsApi> = {}): SettingsApi {
  return {
    load: vi.fn().mockResolvedValue(DEFAULT_SETTINGS),
    save: vi.fn().mockImplementation(async (settings) => ({
      ...DEFAULT_SETTINGS,
      ...settings,
    })),
    sync: vi.fn().mockResolvedValue(EMPTY_SYNC_RESULT),
    getSyncVersions: vi.fn().mockResolvedValue([]),
    getSystemTheme: vi.fn().mockResolvedValue("dark"),
    getSystemLocale: vi.fn().mockResolvedValue("en"),
    ...overrides,
  };
}

function renderApp(api: SettingsApi) {
  return render(
    <SettingsProvider initialSettings={DEFAULT_SETTINGS} api={api}>
      <ThemeContext.Provider
        value={{ theme: "dark", accentPreset: "sky", setTheme: vi.fn() }}
      >
        <LanguageContext.Provider
          value={{ language: "en", setLanguage: vi.fn() }}
        >
          <App />
        </LanguageContext.Provider>
      </ThemeContext.Provider>
    </SettingsProvider>,
  );
}

function installWindowListeners() {
  const callbacks = new Map<string, (event: { payload: unknown }) => unknown>();
  getTauriMocks().listen.mockImplementation(
    async (
      eventName: string,
      callback: (event: { payload: unknown }) => unknown,
    ) => {
      callbacks.set(eventName, callback);
      return () => callbacks.delete(eventName);
    },
  );
  return callbacks;
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason?: unknown) => void;
  const promise = new Promise<T>((resolvePromise, rejectPromise) => {
    resolve = resolvePromise;
    reject = rejectPromise;
  });
  return { promise, resolve, reject };
}

const snippetSummary = {
  id: "snippet-1",
  title: "Lazy detail",
  language: "typescript",
  description: "",
  tags: [],
  is_favorite: false,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  content_preview: "preview only",
};

const snippetDetail = {
  ...snippetSummary,
  content: "full private body",
};

describe("App synchronization coordination", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
    if (!document.getElementById("root")) {
      const root = document.createElement("div");
      root.id = "root";
      document.body.appendChild(root);
    }
  });

  it("refreshes snippets and settings for background completion without a modal", async () => {
    const callbacks = installWindowListeners();
    const api = makeApi();
    const invoke = getTauriMocks().invoke;
    invoke.mockImplementation(async (command: string) => {
      if (command === "query_snippets")
        return { items: [], next_cursor: null, total: 0 };
      if (command === "get_snippet_tags") return [];
      return undefined;
    });
    renderApp(api);

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("query_snippets", expect.any(Object)),
    );
    await waitFor(() => expect(callbacks.has("sync-complete")).toBe(true));
    invoke.mockClear();

    const completion: SyncCompletionEvent = {
      source: "background",
      status: "result",
      result: EMPTY_SYNC_RESULT,
    };
    await act(async () => {
      await callbacks.get("sync-complete")!({ payload: completion });
    });

    expect(
      invoke.mock.calls.filter(([command]) => command === "query_snippets"),
    ).toHaveLength(1);
    expect(invoke).toHaveBeenCalledWith("query_snippets", expect.any(Object));
    expect(api.load).toHaveBeenCalledTimes(1);
    expect(api.getSyncVersions).toHaveBeenCalledTimes(1);
    expect(screen.queryByText("sync done")).not.toBeInTheDocument();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Background sync completed and the latest snippets were refreshed.",
    );
  });

  it("runs Settings sync once and refreshes the main snippet list once", async () => {
    installWindowListeners();
    const api = makeApi();
    const invoke = getTauriMocks().invoke;
    invoke.mockImplementation(async (command: string) => {
      if (command === "query_snippets")
        return { items: [], next_cursor: null, total: 0 };
      if (command === "get_snippet_tags") return [];
      return undefined;
    });
    renderApp(api);
    const user = userEvent.setup();

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("query_snippets", expect.any(Object)),
    );
    invoke.mockClear();
    await user.click(screen.getByTitle("Settings"));
    await user.click(await screen.findByRole("button", { name: "Sync Now" }));
    await user.click(await screen.findByRole("button", { name: "OK" }));

    await waitFor(() => expect(api.sync).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith("query_snippets", expect.any(Object)),
    );
    expect(
      invoke.mock.calls.filter(([command]) => command === "query_snippets"),
    ).toHaveLength(1);
    expect(api.load).toHaveBeenCalledTimes(1);
    expect(api.getSyncVersions).toHaveBeenCalledTimes(1);
    expect(screen.getByText("sync done")).toHaveAttribute("role", "status");
    expect(screen.queryAllByText("sync done")).toHaveLength(1);
  });

  it("loads full detail only after selecting a summary", async () => {
    installWindowListeners();
    const invoke = getTauriMocks().invoke;
    invoke.mockImplementation(async (command: string) => {
      if (command === "query_snippets") {
        return { items: [snippetSummary], next_cursor: null, total: 1 };
      }
      if (command === "get_snippet_tags") return [];
      if (command === "get_snippet") return snippetDetail;
      return undefined;
    });
    renderApp(makeApi());
    const user = userEvent.setup();

    const cardTitle = await screen.findByText("Lazy detail");
    expect(
      invoke.mock.calls.filter(([command]) => command === "get_snippet"),
    ).toHaveLength(0);
    await user.click(cardTitle);
    await waitFor(() => {
      expect(invoke).toHaveBeenCalledWith("get_snippet", { id: "snippet-1" });
    });
    expect(await screen.findByTestId("snippet-editor")).toBeInTheDocument();
  });

  it("does not let a stale detail response replace a newer selection", async () => {
    installWindowListeners();
    const invoke = getTauriMocks().invoke;
    const firstDetail = deferred<typeof snippetDetail>();
    const secondSummary = {
      ...snippetSummary,
      id: "snippet-2",
      title: "Second summary",
      content_preview: "second preview",
    };
    const secondDetail = {
      ...snippetDetail,
      ...secondSummary,
      content: "second full body",
    };
    invoke.mockImplementation(async (command: string, args?: unknown) => {
      if (command === "query_snippets") {
        return {
          items: [snippetSummary, secondSummary],
          next_cursor: null,
          total: 2,
        };
      }
      if (command === "get_snippet_tags") return [];
      if (command === "get_snippet") {
        const id = (args as { id: string }).id;
        return id === snippetSummary.id ? firstDetail.promise : secondDetail;
      }
      return undefined;
    });
    renderApp(makeApi());
    const user = userEvent.setup();

    await user.click(await screen.findByText("Lazy detail"));
    expect(screen.getByText("Loading snippet...")).toBeInTheDocument();
    await user.click(screen.getByText("Second summary"));
    expect(await screen.findByTestId("editor-title")).toHaveTextContent(
      "Second summary",
    );

    await act(async () => {
      firstDetail.resolve(snippetDetail);
      await firstDetail.promise;
    });
    expect(screen.getByTestId("editor-title")).toHaveTextContent(
      "Second summary",
    );
  });

  it("preserves dirty selected detail during an authoritative refresh", async () => {
    const callbacks = installWindowListeners();
    const invoke = getTauriMocks().invoke;
    let detail = snippetDetail;
    invoke.mockImplementation(async (command: string) => {
      if (command === "query_snippets") {
        return { items: [snippetSummary], next_cursor: null, total: 1 };
      }
      if (command === "get_snippet_tags") return [];
      if (command === "get_snippet") return detail;
      return undefined;
    });
    renderApp(makeApi());
    const user = userEvent.setup();

    await user.click(await screen.findByText("Lazy detail"));
    expect(await screen.findByTestId("editor-title")).toHaveTextContent(
      "Lazy detail",
    );
    await user.click(screen.getByRole("button", { name: "Make dirty" }));
    expect(screen.getByTestId("editor-title")).toHaveTextContent(
      "Dirty local title",
    );
    detail = { ...snippetDetail, title: "Remote title" };

    await act(async () => {
      await callbacks.get("sync-complete")!({
        payload: {
          source: "background",
          status: "result",
          result: EMPTY_SYNC_RESULT,
        } satisfies SyncCompletionEvent,
      });
    });

    expect(screen.getByTestId("editor-title")).toHaveTextContent(
      "Dirty local title",
    );
    expect(
      screen.getByText(
        "New saved data is available. Your unsaved edits were preserved.",
      ),
    ).toHaveAttribute("role", "status");
  });

  it("keeps native context menus outside editable text and navigates the custom menu", async () => {
    installWindowListeners();
    const invoke = getTauriMocks().invoke;
    invoke.mockImplementation(async (command: string) => {
      if (command === "query_snippets")
        return { items: [], next_cursor: null, total: 0 };
      if (command === "get_snippet_tags") return [];
      return undefined;
    });
    renderApp(makeApi());
    const user = userEvent.setup();

    const settingsButton = await screen.findByRole("button", {
      name: "Settings",
    });
    const nativeMenuEvent = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
    });
    settingsButton.dispatchEvent(nativeMenuEvent);
    expect(nativeMenuEvent.defaultPrevented).toBe(false);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    const search = screen.getByPlaceholderText("Search snippets...");
    search.focus();
    const customMenuEvent = new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      clientX: 24,
      clientY: 24,
    });
    search.dispatchEvent(customMenuEvent);
    expect(customMenuEvent.defaultPrevented).toBe(true);

    const menu = await screen.findByRole("menu");
    const items = screen.getAllByRole("menuitem");
    await waitFor(() => expect(items[0]).toHaveFocus());
    await user.keyboard("{ArrowDown}");
    expect(items[1]).toHaveFocus();
    await user.keyboard("{End}");
    expect(items.at(-1)).toHaveFocus();
    await user.keyboard("{Home}");
    expect(items[0]).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(items.at(-1)).toHaveFocus();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(menu).not.toBeInTheDocument());
    expect(search).toHaveFocus();
  });

  it("shows a retryable selected-detail error", async () => {
    installWindowListeners();
    const invoke = getTauriMocks().invoke;
    let detailAttempts = 0;
    invoke.mockImplementation(async (command: string) => {
      if (command === "query_snippets") {
        return { items: [snippetSummary], next_cursor: null, total: 1 };
      }
      if (command === "get_snippet_tags") return [];
      if (command === "get_snippet") {
        detailAttempts += 1;
        if (detailAttempts === 1) {
          throw { code: "database", message: "safe", retryable: true };
        }
        return snippetDetail;
      }
      return undefined;
    });
    renderApp(makeApi());
    const user = userEvent.setup();

    await user.click(await screen.findByText("Lazy detail"));
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The snippet could not be loaded",
    );
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByTestId("snippet-editor")).toBeInTheDocument();
    expect(detailAttempts).toBe(2);
  });
});
