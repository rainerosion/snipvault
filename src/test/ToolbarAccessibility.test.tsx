import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { Toolbar } from "../components/Toolbar";
import i18n from "../i18n";

const PROPS = {
  searchQuery: "query",
  onSearchChange: vi.fn(),
  selectedLang: "",
  onLangChange: vi.fn(),
  onNew: vi.fn(),
  onExport: vi.fn(),
  onImportData: vi.fn(),
  onImportError: vi.fn(),
  theme: "dark" as const,
  onThemeToggle: vi.fn(),
  onFavoriteFilter: vi.fn(),
  onOpenSettings: vi.fn(),
  onSync: vi.fn(),
  syncing: false,
  favoriteFilter: null,
  totalCount: 3,
};

describe("Toolbar accessible controls", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("names icon buttons and exposes toggle state", () => {
    render(<Toolbar {...PROPS} />);

    for (const name of [
      "Clear search",
      "Favorites only",
      "Export (Ctrl+E)",
      "Import",
      "Sync",
      "Settings",
      "Toggle theme",
      "New",
    ]) {
      const button = screen.getByRole("button", { name });
      expect(button).toHaveAttribute("type", "button");
    }

    expect(
      screen.getByRole("button", { name: "Favorites only" }),
    ).toHaveAttribute("aria-pressed", "false");
    expect(
      screen.getByRole("button", { name: "Toggle theme" }),
    ).toHaveAttribute("aria-pressed", "false");
  });
});
