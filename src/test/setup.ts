import "@testing-library/jest-dom/vitest";

import { cleanup } from "@testing-library/react";
import { toHaveNoViolations } from "jest-axe";
import { afterEach, beforeEach, expect, vi } from "vitest";

expect.extend(toHaveNoViolations);

Object.defineProperty(window, "matchMedia", {
  configurable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

const tauriMocks = vi.hoisted(() => ({
  close: vi.fn().mockResolvedValue(undefined),
  emit: vi.fn().mockResolvedValue(undefined),
  emitTo: vi.fn().mockResolvedValue(undefined),
  invoke: vi.fn().mockResolvedValue(undefined),
  isMaximized: vi.fn().mockResolvedValue(false),
  listen: vi.fn().mockResolvedValue(() => undefined),
  maximize: vi.fn().mockResolvedValue(undefined),
  minimize: vi.fn().mockResolvedValue(undefined),
  onResized: vi.fn().mockResolvedValue(() => undefined),
  once: vi.fn().mockResolvedValue(() => undefined),
  readText: vi.fn().mockResolvedValue(""),
  unmaximize: vi.fn().mockResolvedValue(undefined),
  writeText: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@tauri-apps/api/core", () => ({
  invoke: tauriMocks.invoke,
}));

vi.mock("@tauri-apps/api/event", () => ({
  emit: tauriMocks.emit,
  emitTo: tauriMocks.emitTo,
  listen: tauriMocks.listen,
  once: tauriMocks.once,
}));

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  readText: tauriMocks.readText,
  writeText: tauriMocks.writeText,
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    close: tauriMocks.close,
    isMaximized: tauriMocks.isMaximized,
    listen: tauriMocks.listen,
    maximize: tauriMocks.maximize,
    minimize: tauriMocks.minimize,
    onResized: tauriMocks.onResized,
    unmaximize: tauriMocks.unmaximize,
  }),
}));

export function getTauriMocks() {
  return tauriMocks;
}

beforeEach(() => {
  tauriMocks.isMaximized.mockResolvedValue(false);
  tauriMocks.onResized.mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  tauriMocks.isMaximized.mockResolvedValue(false);
  tauriMocks.onResized.mockResolvedValue(() => undefined);
  tauriMocks.onResized.mockClear();
});
