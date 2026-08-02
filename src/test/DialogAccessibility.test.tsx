import { createRef, useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
import { beforeEach, describe, expect, it } from "vitest";
import { Dialog, type DialogHandle } from "../components/Dialog";
import { ModalSurface } from "../components/ModalSurface";
import i18n from "../i18n";

function NestedModalHarness() {
  const dialogRef = createRef<DialogHandle>();
  const [settingsOpen, setSettingsOpen] = useState(false);

  return (
    <>
      <button type="button" onClick={() => setSettingsOpen(true)}>
        Open settings
      </button>
      {settingsOpen && (
        <div className="settings-overlay">
          <ModalSurface
            className="settings-panel"
            labelledBy="nested-settings-title"
            onEscape={() => setSettingsOpen(false)}
          >
            <h2 id="nested-settings-title">Settings</h2>
            <button
              type="button"
              onClick={() => void dialogRef.current?.confirm("Continue?")}
            >
              Open confirm
            </button>
            <button type="button" onClick={() => setSettingsOpen(false)}>
              Close settings
            </button>
            <Dialog ref={dialogRef} theme="dark" />
          </ModalSurface>
        </div>
      )}
    </>
  );
}

describe("Dialog focus management", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("traps focus in the topmost nested dialog and restores focus in order", async () => {
    const user = userEvent.setup();
    render(<NestedModalHarness />);

    const opener = screen.getByRole("button", { name: "Open settings" });
    await user.click(opener);
    const nestedOpener = screen.getByRole("button", { name: "Open confirm" });
    nestedOpener.focus();
    await user.click(nestedOpener);

    const confirmation = await screen.findByRole("dialog", { name: "Confirm" });
    const cancel = screen.getByRole("button", { name: "Cancel" });
    expect(cancel).toHaveFocus();

    await user.tab({ shift: true });
    expect(screen.getByRole("button", { name: "OK" })).toHaveFocus();
    await user.keyboard("{Escape}");
    await waitFor(() => expect(confirmation).not.toBeInTheDocument());
    expect(nestedOpener).toHaveFocus();

    await user.click(screen.getByRole("button", { name: "Close settings" }));
    expect(opener).toHaveFocus();
  });

  it("labels alert dialogs and has no detectable axe violations", async () => {
    const ref = createRef<DialogHandle>();
    const { container } = render(<Dialog ref={ref} theme="light" />);
    void ref.current?.alert("Something happened", "Notice");

    const dialog = await screen.findByRole("alertdialog", { name: "Notice" });
    expect(dialog).toHaveAccessibleDescription("Something happened");
    expect(await axe(container)).toHaveNoViolations();
  });
});
