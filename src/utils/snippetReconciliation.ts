import type { Snippet } from "../types";

export type ReconciliationAction = "none" | "refresh" | "preserve-dirty" | "clear";

export function getReconciliationAction(
  currentTarget: string | null,
  dirty: boolean,
  authoritative: Snippet[]
): ReconciliationAction {
  if (!currentTarget || currentTarget === "new") return "none";
  if (dirty) return "preserve-dirty";
  return authoritative.some((snippet) => snippet.id === currentTarget) ? "refresh" : "clear";
}
