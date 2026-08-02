import { describe, expect, it } from "vitest";
import type { Snippet } from "../types";
import { getReconciliationAction } from "../utils/snippetReconciliation";
import {
  localizeCommandError,
  normalizeCommandError,
} from "../utils/commandErrors";

const snippet: Snippet = {
  id: "snippet-1",
  title: "Authoritative",
  content: "latest",
  language: "plaintext",
  description: "",
  tags: [],
  is_favorite: true,
  created_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-02T00:00:00Z",
  revision_id: "revision-1",
};

describe("command error normalization", () => {
  it("accepts stale revision and outbox capacity errors", () => {
    expect(
      normalizeCommandError({
        code: "stale_revision",
        message: "safe",
        retryable: false,
        details: { current_revision_id: "revision-2" },
      }),
    ).toMatchObject({
      code: "stale_revision",
      details: { current_revision_id: "revision-2" },
    });
    expect(
      normalizeCommandError({
        code: "outbox_full",
        message: "safe",
        retryable: false,
      }),
    ).toMatchObject({ code: "outbox_full", retryable: false });
  });

  it("safely falls back for malformed and unknown structured rejections", () => {
    expect(
      normalizeCommandError({
        code: "credential_dump",
        message: "secret",
        retryable: true,
      }),
    ).toMatchObject({ code: "unknown", retryable: false });
    expect(
      normalizeCommandError("server body contains password=secret"),
    ).toMatchObject({ code: "unknown", retryable: false });

    const t = (key: string, options?: { defaultValue?: string }) =>
      key === "commandErrors.unknown"
        ? "Safe fallback"
        : (options?.defaultValue ?? key);
    expect(localizeCommandError("password=secret", t as never)).toBe(
      "Safe fallback",
    );
  });

  it("accepts a serialized known structured command error", () => {
    expect(
      normalizeCommandError(
        JSON.stringify({ code: "sync_busy", message: "safe", retryable: true }),
      ),
    ).toEqual({ code: "sync_busy", message: "safe", retryable: true });
  });
});

describe("authoritative snippet reconciliation", () => {
  it("refreshes a clean selected form from authoritative data", () => {
    expect(getReconciliationAction(snippet.id, false, [snippet])).toBe(
      "refresh",
    );
  });

  it("never overwrites a dirty editor after an authoritative reload", () => {
    expect(getReconciliationAction(snippet.id, true, [snippet])).toBe(
      "preserve-dirty",
    );
  });

  it("clears a clean selection deleted from authoritative data", () => {
    expect(getReconciliationAction(snippet.id, false, [])).toBe("clear");
  });
});
