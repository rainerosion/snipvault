import { useState } from "react";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { Sidebar } from "../components/Sidebar";
import type { SnippetSummary } from "../types";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, fallback?: string) => fallback ?? key,
  }),
}));

const snippet: SnippetSummary = {
  id: "snippet-1",
  title: "Loaded snippet",
  content_preview: "const loaded = true;",
  language: "javascript",
  description: "",
  tags: [],
  is_favorite: false,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  revision_id: "revision-1",
};

const noop = () => undefined;

describe("snippet load reliability states", () => {
  it("distinguishes an initial load error from an empty database", () => {
    const { rerender } = render(
      <Sidebar
        snippets={[]}
        selectedId={null}
        onSelect={noop}
        onDelete={noop}
        onToggleFavorite={noop}
        loading={false}
        loadingMore={false}
        hasMore={false}
        error="The local database operation failed."
        loadMoreError={null}
        onRetry={noop}
        onLoadMore={noop}
      />,
    );

    expect(screen.getByRole("alert")).toHaveTextContent(
      "The local database operation failed.",
    );
    expect(screen.queryByText("sidebar.empty")).not.toBeInTheDocument();

    rerender(
      <Sidebar
        snippets={[]}
        selectedId={null}
        onSelect={noop}
        onDelete={noop}
        onToggleFavorite={noop}
        loading={false}
        loadingMore={false}
        hasMore={false}
        error={null}
        loadMoreError={null}
        onRetry={noop}
        onLoadMore={noop}
      />,
    );

    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByText("sidebar.empty")).toBeInTheDocument();
  });

  it("retries a failed load and renders the authoritative success data", async () => {
    const user = userEvent.setup();
    const load = vi.fn().mockResolvedValue([snippet]);

    function Harness() {
      const [snippets, setSnippets] = useState<SnippetSummary[]>([]);
      const [error, setError] = useState<string | null>("Load failed");
      return (
        <Sidebar
          snippets={snippets}
          selectedId={null}
          onSelect={noop}
          onDelete={noop}
          onToggleFavorite={noop}
          loading={false}
          loadingMore={false}
          hasMore={false}
          error={error}
          loadMoreError={null}
          onRetry={() => {
            void load().then((loaded: SnippetSummary[]) => {
              setSnippets(loaded);
              setError(null);
            });
          }}
          onLoadMore={noop}
        />
      );
    }

    render(<Harness />);
    await user.click(screen.getByRole("button", { name: "sidebar.retry" }));

    expect(load).toHaveBeenCalledOnce();
    expect(await screen.findByText("Loaded snippet")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});
