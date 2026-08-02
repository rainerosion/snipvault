import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { axe } from "jest-axe";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SnippetList } from "../components/SnippetList";
import { LanguageContext } from "../context/LanguageContext";
import i18n from "../i18n";
import type { SnippetSummary } from "../types";

const SNIPPETS: SnippetSummary[] = [
  {
    id: "one",
    title: "Fetch profile",
    description: "Request a profile",
    language: "typescript",
    tags: ["http"],
    is_favorite: false,
    created_at: "2026-01-01T00:00:00Z",
    updated_at: "2026-01-01T00:00:00Z",
    revision_id: "revision-1",
    content_preview: "const profile = await fetch('/api/profile');",
  },
];

function Harness({
  onSelect = vi.fn(),
}: {
  onSelect?: (snippet: SnippetSummary) => void;
}) {
  const [favorite, setFavorite] = useState(false);
  const snippets = SNIPPETS.map((snippet) => ({
    ...snippet,
    is_favorite: favorite,
  }));

  return (
    <LanguageContext.Provider value={{ language: "en", setLanguage: vi.fn() }}>
      <SnippetList
        snippets={snippets}
        selectedId={null}
        onSelect={onSelect}
        onDelete={vi.fn()}
        onToggleFavorite={() => setFavorite((value) => !value)}
        loading={false}
        loadingMore={false}
        hasMore={false}
        error={null}
        loadMoreError={null}
        onRetry={vi.fn()}
        onLoadMore={vi.fn()}
      />
    </LanguageContext.Provider>
  );
}

describe("SnippetList accessibility", () => {
  beforeEach(async () => {
    await i18n.changeLanguage("en");
  });

  it("uses semantic list items and native keyboard buttons without nesting actions", async () => {
    const user = userEvent.setup();
    const onSelect = vi.fn();
    render(<Harness onSelect={onSelect} />);

    const list = screen.getByRole("list", { name: "Code snippets" });
    expect(list).toBeInTheDocument();
    expect(screen.getAllByRole("listitem")).toHaveLength(1);

    const selection = screen.getByRole("button", {
      name: "Select snippet Fetch profile",
    });
    selection.focus();
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(onSelect).toHaveBeenCalledTimes(2);
    expect(selection.querySelector("button")).toBeNull();

    const favorite = screen.getByRole("button", {
      name: "Favorite Fetch profile",
    });
    expect(favorite).toHaveAttribute("aria-pressed", "false");
    await user.click(favorite);
    expect(
      screen.getByRole("button", {
        name: "Remove Fetch profile from favorites",
      }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      screen.getByRole("button", { name: "Delete Fetch profile" }),
    ).toBeInTheDocument();
  });

  it("has no detectable axe violations", async () => {
    const { container } = render(<Harness />);
    expect(await axe(container)).toHaveNoViolations();
  });
});
