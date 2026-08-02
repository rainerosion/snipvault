import { act, renderHook, waitFor } from "@testing-library/react";
import { createElement, type ReactNode } from "react";
import { describe, expect, it, vi } from "vitest";
import {
  SettingsProvider,
  settingsToDraft,
  useSettings,
  type Settings,
  type SettingsApi,
} from "../hooks/useSettings";
import { DEFAULT_SETTINGS } from "./settingsFixtures";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolver) => {
    resolve = resolver;
  });
  return { promise, resolve };
}

function makeApi(overrides: Partial<SettingsApi> = {}): SettingsApi {
  return {
    load: vi.fn().mockResolvedValue(DEFAULT_SETTINGS),
    save: vi.fn().mockImplementation(async (settings) => ({
      ...DEFAULT_SETTINGS,
      ...settings,
    })),
    sync: vi.fn().mockResolvedValue({
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
    }),
    getSyncVersions: vi.fn().mockResolvedValue([]),
    getSystemTheme: vi.fn().mockResolvedValue("dark"),
    getSystemLocale: vi.fn().mockResolvedValue("en"),
    ...overrides,
  };
}

describe("SettingsProvider", () => {
  it("shares one authoritative state across consumers", async () => {
    const api = makeApi({
      save: vi.fn().mockResolvedValue({
        ...DEFAULT_SETTINGS,
        editor_line_wrap: false,
      }),
    });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(
        SettingsProvider,
        { initialSettings: DEFAULT_SETTINGS, api },
        children,
      );
    const { result } = renderHook(
      () => [useSettings(), useSettings()] as const,
      { wrapper },
    );

    await waitFor(() => expect(result.current[0].loading).toBe(false));
    await act(async () => {
      await result.current[0].save({
        ...settingsToDraft(DEFAULT_SETTINGS),
        editor_line_wrap: false,
      });
    });

    expect(result.current[0].settings?.editor_line_wrap).toBe(false);
    expect(result.current[1].settings?.editor_line_wrap).toBe(false);
  });

  it("sends an explicit tagged secret action without exposing a stored value", async () => {
    const save = vi.fn().mockResolvedValue({
      ...DEFAULT_SETTINGS,
      webdav_secret_configured: true,
    });
    const api = makeApi({ save });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(
        SettingsProvider,
        { initialSettings: DEFAULT_SETTINGS, api },
        children,
      );
    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => expect(result.current.loading).toBe(false));
    await act(async () => {
      await result.current.save(settingsToDraft(DEFAULT_SETTINGS), {
        action: "replace",
        value: "replacement-value",
      });
    });

    expect(save).toHaveBeenCalledWith(settingsToDraft(DEFAULT_SETTINGS), {
      action: "replace",
      value: "replacement-value",
    });
    expect(result.current.settings).not.toHaveProperty("webdav_password");
  });

  it("does not let an older reload overwrite a successful save", async () => {
    const pendingReload = deferred<Settings>();
    const saved: Settings = { ...DEFAULT_SETTINGS, language: "zh" };
    const api = makeApi({
      load: vi.fn().mockReturnValue(pendingReload.promise),
      save: vi.fn().mockResolvedValue(saved),
    });
    const wrapper = ({ children }: { children: ReactNode }) =>
      createElement(
        SettingsProvider,
        { initialSettings: DEFAULT_SETTINGS, api },
        children,
      );
    const { result } = renderHook(() => useSettings(), { wrapper });

    await waitFor(() => expect(result.current.loading).toBe(false));
    let reloadPromise!: Promise<Settings>;
    act(() => {
      reloadPromise = result.current.reload();
    });
    await act(async () => {
      await result.current.save(settingsToDraft(saved));
    });
    await act(async () => {
      pendingReload.resolve(DEFAULT_SETTINGS);
      await reloadPromise;
    });

    expect(result.current.settings?.language).toBe("zh");
  });
});
