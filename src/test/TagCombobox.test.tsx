import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
import { beforeEach, describe, expect, it } from "vitest";
import { SnippetTagCombobox } from "../components/SnippetEditor";
import i18n from "../i18n";

function Harness() {
  const [tags, setTags] = useState<string[]>([]);
  return (
    <SnippetTagCombobox
      tags={tags}
      tagOptions={["react", "rust", "typescript"]}
      onChange={setTags}
    />
  );
}

describe("Snippet tag combobox", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("navigates suggestions and selects the active option before raw creation", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const combobox = screen.getByRole("combobox", { name: /Type a tag/i });

    await user.click(combobox);
    expect(combobox).toHaveAttribute("aria-expanded", "true");
    expect(screen.getAllByRole("option")[0]).toHaveAttribute(
      "aria-selected",
      "true",
    );

    await user.keyboard("{ArrowDown}{Enter}");
    expect(
      screen.getByRole("button", { name: "Remove tag rust" }),
    ).toBeInTheDocument();
    expect(combobox).toHaveValue("");

    await user.type(combobox, "custom{Enter}");
    expect(
      screen.getByRole("button", { name: "Remove tag custom" }),
    ).toBeInTheDocument();
  });

  it("closes on Escape and keeps mouse selection coordinated without a blur timer", async () => {
    const user = userEvent.setup();
    render(<Harness />);
    const combobox = screen.getByRole("combobox", { name: /Type a tag/i });

    await user.click(combobox);
    await user.keyboard("{Escape}");
    expect(combobox).toHaveAttribute("aria-expanded", "false");

    await user.click(combobox);
    await user.click(screen.getByRole("option", { name: "react" }));
    expect(
      screen.getByRole("button", { name: "Remove tag react" }),
    ).toBeInTheDocument();
    expect(combobox).toHaveFocus();
  });

  it("has no detectable axe violations", async () => {
    const { container } = render(<Harness />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
