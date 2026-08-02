import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
import { beforeEach, describe, expect, it, vi } from "vitest";

import { Titlebar } from "../components/Titlebar";
import { getTauriMocks } from "./setup";

const tauriMocks = getTauriMocks();

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string) =>
      ({
        "app.title": "SnipVault",
        "titlebar.close": "Close",
        "titlebar.maximize": "Maximize",
        "titlebar.minimize": "Minimize",
        "titlebar.restore": "Restore",
      })[key] ?? key,
  }),
}));

describe("Titlebar", () => {
  beforeEach(() => {
    tauriMocks.isMaximized.mockResolvedValue(false);
  });

  it("drives native window controls and cleans up its resize listener", async () => {
    const unlisten = vi.fn();
    let resizeHandler: (() => void) | undefined;
    tauriMocks.onResized.mockImplementation(async (handler: () => void) => {
      resizeHandler = handler;
      return unlisten;
    });

    const user = userEvent.setup();
    const { unmount } = render(<Titlebar theme="dark" />);

    await waitFor(() => expect(tauriMocks.onResized).toHaveBeenCalledOnce());

    await user.click(screen.getByRole("button", { name: "Minimize" }));
    expect(tauriMocks.minimize).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Maximize" }));
    expect(tauriMocks.maximize).toHaveBeenCalledOnce();

    tauriMocks.isMaximized.mockResolvedValue(true);
    resizeHandler?.();
    const restoreButton = await screen.findByRole("button", {
      name: "Restore",
    });
    await user.click(restoreButton);
    expect(tauriMocks.unmaximize).toHaveBeenCalledOnce();

    await user.click(screen.getByRole("button", { name: "Close" }));
    expect(tauriMocks.close).toHaveBeenCalledOnce();

    unmount();
    await waitFor(() => expect(unlisten).toHaveBeenCalledOnce());
  });

  it("has no detectable axe violations", async () => {
    const { container } = render(<Titlebar theme="light" />);

    expect(await axe(container)).toHaveNoViolations();
  });
});
