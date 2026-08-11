import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { createElement, createRef, type ReactNode } from "react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import i18n from "../i18n";
import { LanguageContext } from "../context/LanguageContext";
import {
  SettingsPanel,
  type SettingsPanelHandle,
} from "../components/Settings";
import {
  SettingsProvider,
  type Settings,
  type SettingsApi,
  type SettingsContextValue,
  type SyncCompletionEvent,
  useSettings,
} from "../hooks/useSettings";
import { DEFAULT_SETTINGS } from "./settingsFixtures";

function makeApi(overrides: Partial<SettingsApi> = {}): SettingsApi {
  return {
    load: vi.fn().mockResolvedValue(DEFAULT_SETTINGS),
    save: vi.fn().mockImplementation(async (settings) => ({
      ...DEFAULT_SETTINGS,
      ...settings,
    })),
    sync: vi.fn(),
    getSyncVersions: vi.fn().mockResolvedValue([]),
    getSystemTheme: vi.fn().mockResolvedValue("dark"),
    getSystemLocale: vi.fn().mockResolvedValue("en"),
    ...overrides,
  };
}

function renderPanel(
  options: {
    api?: SettingsApi;
    initialSettings?: Settings;
    onClose?: () => void;
    onSync?: () => Promise<SyncCompletionEvent>;
    panelRef?: ReturnType<typeof createRef<SettingsPanelHandle>>;
  } = {},
) {
  const api = options.api ?? makeApi();
  const onClose = options.onClose ?? vi.fn();
  const onSync =
    options.onSync ??
    vi.fn().mockResolvedValue({
      source: "settings",
      status: "result",
      result: {
        success: true,
        message: "done",
        uploaded: false,
        uploaded_count: 0,
        downloaded_count: 0,
        deleted_count: 0,
        conflict_count: 0,
        pending_count: 0,
        protocol_version: 2,
        manifest_generation: 0,
        total_count: 0,
      },
    });
  let settingsContext: SettingsContextValue | null = null;
  const ContextCapture = () => {
    settingsContext = useSettings();
    return null;
  };
  const Wrapper = ({ children }: { children: ReactNode }) =>
    createElement(
      SettingsProvider,
      {
        initialSettings: options.initialSettings ?? DEFAULT_SETTINGS,
        api,
      },
      createElement(
        LanguageContext.Provider,
        { value: { language: "en", setLanguage: vi.fn() } },
        createElement(ContextCapture),
        children,
      ),
    );

  return {
    api,
    onClose,
    onSync,
    getSettingsContext: () => settingsContext,
    ...render(
      <SettingsPanel
        ref={options.panelRef}
        onClose={onClose}
        onSync={onSync}
      />,
      { wrapper: Wrapper },
    ),
  };
}

describe("SettingsPanel draft behavior", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });
  it("keeps dirty fields and announces external authoritative changes", async () => {
    let resolveReload!: (settings: Settings) => void;
    const reload = new Promise<Settings>((resolve) => {
      resolveReload = resolve;
    });
    const api = makeApi({ load: vi.fn().mockReturnValue(reload) });
    const { getSettingsContext } = renderPanel({ api });
    const user = userEvent.setup();

    const username = await screen.findByLabelText("Username");
    await user.clear(username);
    await user.type(username, "draft-user");

    let reloadPromise!: Promise<Settings>;
    act(() => {
      reloadPromise = getSettingsContext()!.reload();
    });
    await act(async () => {
      resolveReload({
        ...DEFAULT_SETTINGS,
        theme: "dark",
        last_sync_at: "2026-02-01T00:00:00Z",
      });
      await reloadPromise;
    });
    await waitFor(() => {
      expect(
        screen.getByText(/Saved settings changed elsewhere/),
      ).toBeInTheDocument();
    });
    expect(username).toHaveValue("draft-user");
  });

  it("guards Escape with save/discard/cancel and stays open on save failure", async () => {
    const onClose = vi.fn();
    const api = makeApi({
      save: vi.fn().mockRejectedValue({
        code: "settings",
        message: "safe",
        retryable: true,
      }),
    });
    renderPanel({ api, onClose });
    const user = userEvent.setup();

    await user.click(
      await screen.findByRole("checkbox", { name: /Start with Windows/i }),
    );
    const closeButton = screen.getByRole("button", { name: "Close" });
    closeButton.focus();
    await user.keyboard("{Escape}");
    const guard = await screen.findByRole("dialog", { name: "Confirm" });
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(guard).not.toBeInTheDocument());
    expect(closeButton).toHaveFocus();

    await user.keyboard("{Escape}");
    await user.click(await screen.findByRole("button", { name: "Save" }));

    await screen.findByText(/Settings save failed/i);
    expect(onClose).not.toHaveBeenCalled();
    expect(
      screen.getByRole("dialog", { name: "Settings" }),
    ).toBeInTheDocument();
  });

  it("routes the close button through the discard guard", async () => {
    const onClose = vi.fn();
    renderPanel({ onClose });
    const user = userEvent.setup();

    await user.click(await screen.findByLabelText("Username"));
    await user.keyboard("-draft");
    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).not.toHaveBeenCalled();

    await user.click(await screen.findByRole("button", { name: "Discard" }));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("exposes the same discard guard for backdrop close requests", async () => {
    const onClose = vi.fn();
    const panelRef = createRef<SettingsPanelHandle>();
    renderPanel({ onClose, panelRef });
    const user = userEvent.setup();

    await user.click(await screen.findByLabelText("Username"));
    await user.keyboard("-draft");
    const closePromise = panelRef.current!.requestClose();
    await screen.findByRole("dialog", { name: "Confirm" });
    expect(onClose).not.toHaveBeenCalled();

    await user.click(await screen.findByRole("button", { name: "Discard" }));
    await expect(closePromise).resolves.toBe(true);
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("disables Save when clean and Sync Now when WebDAV draft is dirty", async () => {
    renderPanel();
    const user = userEvent.setup();

    const save = await screen.findByRole("button", { name: "Save Settings" });
    const sync = screen.getByRole("button", { name: "Sync Now" });
    expect(save).toBeDisabled();
    expect(sync).toBeEnabled();

    await user.type(screen.getByLabelText("Username"), "-changed");
    expect(save).toBeEnabled();
    expect(sync).toBeDisabled();
    expect(
      screen.getByText("Save settings before syncing."),
    ).toBeInTheDocument();
  });

  it("starts the credential field blank and sends replace or clear intent", async () => {
    const save = vi.fn().mockResolvedValue(DEFAULT_SETTINGS);
    const api = makeApi({ save });
    renderPanel({ api });
    const user = userEvent.setup();

    const credential = await screen.findByLabelText("Password / API Key");
    expect(credential).toHaveValue("");
    expect(credential).toHaveAttribute(
      "placeholder",
      "Stored securely — enter to replace",
    );

    await user.type(credential, "replacement-value");
    await user.click(screen.getByRole("button", { name: "Save Settings" }));
    expect(save).toHaveBeenLastCalledWith(expect.any(Object), {
      action: "replace",
      value: "replacement-value",
    });

    await user.click(
      screen.getByRole("button", { name: "Clear stored credential" }),
    );
    await user.click(screen.getByRole("button", { name: "Save Settings" }));
    expect(save).toHaveBeenLastCalledWith(expect.any(Object), {
      action: "clear",
    });
  });

  it("renders v2 sync history counts, protocol metadata, and loading/error states", async () => {
    let rejectHistory!: (reason?: unknown) => void;
    const pendingHistory = new Promise<never>((_, reject) => {
      rejectHistory = reject;
    });
    const api = makeApi({
      getSyncVersions: vi
        .fn()
        .mockReturnValueOnce(pendingHistory)
        .mockResolvedValueOnce([
          {
            id: "history-1",
            synced_at: "2026-02-01T00:00:00Z",
            direction: "publish",
            snippet_count: 5,
            uploaded_count: 1,
            downloaded_count: 2,
            deleted_count: 1,
            conflict_count: 1,
            protocol_version: 2,
            generation: 7,
            message: "merged with tombstones",
          },
        ]),
    });
    renderPanel({ api });
    const user = userEvent.setup();

    await user.click(
      await screen.findByRole("button", { name: "Sync History" }),
    );
    expect(
      await screen.findByText("Loading sync history..."),
    ).toBeInTheDocument();
    await act(async () => {
      rejectHistory({ code: "database", message: "safe", retryable: true });
    });
    expect(
      await screen.findByText(
        "The local database operation failed. Please retry.",
      ),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Collapse" }));
    await user.click(screen.getByRole("button", { name: "Sync History" }));

    expect(await screen.findByText("Published")).toBeInTheDocument();
    expect(
      screen.getByText("5 total · 1 up · 2 down · 1 deleted · 1 conflicts"),
    ).toBeInTheDocument();
    expect(screen.getByText("Protocol v2 · generation 7")).toBeInTheDocument();
    expect(screen.getByText("merged with tombstones")).toBeInTheDocument();
  });

  it("runs settings sync through the injected coordinator", async () => {
    const onSync = vi.fn().mockResolvedValue({
      source: "settings",
      status: "result",
      result: {
        success: true,
        message: "settings sync done",
        uploaded: false,
        uploaded_count: 0,
        downloaded_count: 1,
        deleted_count: 0,
        conflict_count: 0,
        pending_count: 0,
        protocol_version: 2,
        manifest_generation: 3,
        total_count: 2,
      },
    } satisfies SyncCompletionEvent);
    renderPanel({ onSync });
    const user = userEvent.setup();

    await user.click(await screen.findByRole("button", { name: "Sync Now" }));
    await user.click(await screen.findByRole("button", { name: "OK" }));

    await waitFor(() => expect(onSync).toHaveBeenCalledTimes(1));
    const status = screen.getByRole("status");
    expect(status).toHaveTextContent("settings sync done");
    expect(status).toHaveTextContent("Protocol v2 · generation 3");
    expect(status).toHaveTextContent(
      "2 total · 0 up · 1 down · 0 deleted · 0 conflicts · 0 pending",
    );
  });
});
