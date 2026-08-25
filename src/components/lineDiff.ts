export type DiffRowKind = "equal" | "delete" | "insert" | "replace";

export interface DiffSourceLine {
  lineNumber: number;
  text: string;
}

export interface DiffRow {
  kind: DiffRowKind;
  left?: DiffSourceLine;
  right?: DiffSourceLine;
}

export type DiffResult =
  | {
      status: "ready";
      rows: DiffRow[];
      modifications: number;
      additions: number;
      deletions: number;
    }
  | { status: "identical"; lineCount: number }
  | { status: "limited"; reason: "size" | "complexity" | "time" };

const MAX_COMBINED_CHARS = 384 * 1024;
const MAX_COMBINED_LINES = 4_000;
const MAX_MATRIX_CELLS = 4_000_000;
const MAX_RENDERED_ROWS = 8_000;
const WORK_BUDGET_MS = 45;

function splitLines(content: string): string[] {
  return content.split("\n").map((line) => line.endsWith("\r") ? line.slice(0, -1) : line);
}

function elapsed(startedAt: number): number {
  return performance.now() - startedAt;
}

function limited(reason: "size" | "complexity" | "time"): DiffResult {
  return { status: "limited", reason };
}

function appendRun(
  rows: DiffRow[],
  deleted: DiffSourceLine[],
  inserted: DiffSourceLine[],
): { modifications: number; additions: number; deletions: number } {
  let modifications = 0;
  let additions = 0;
  let deletions = 0;
  const paired = Math.max(deleted.length, inserted.length);
  for (let index = 0; index < paired; index += 1) {
    const left = deleted[index];
    const right = inserted[index];
    if (left && right) {
      rows.push({ kind: "replace", left, right });
      modifications += 1;
    } else if (left) {
      rows.push({ kind: "delete", left });
      deletions += 1;
    } else if (right) {
      rows.push({ kind: "insert", right });
      additions += 1;
    }
  }
  return { modifications, additions, deletions };
}

/**
 * Produces a bounded, two-way, line-aligned diff for local revision review.
 * The dynamic-programming matrix is deliberately capped so history inspection
 * cannot block the modal on huge or highly divergent snippet bodies.
 */
export function buildLineDiff(leftContent: string, rightContent: string): DiffResult {
  if (leftContent === rightContent) {
    return { status: "identical", lineCount: splitLines(leftContent).length };
  }

  if (leftContent.length + rightContent.length > MAX_COMBINED_CHARS) {
    return limited("size");
  }

  const startedAt = performance.now();
  const leftLines = splitLines(leftContent);
  const rightLines = splitLines(rightContent);
  const leftCount = leftLines.length;
  const rightCount = rightLines.length;

  if (
    leftCount + rightCount > MAX_COMBINED_LINES
    || (leftCount + 1) * (rightCount + 1) > MAX_MATRIX_CELLS
  ) {
    return limited("size");
  }

  const width = rightCount + 1;
  const matrix = new Int32Array((leftCount + 1) * width);
  for (let left = leftCount - 1; left >= 0; left -= 1) {
    const base = left * width;
    const below = (left + 1) * width;
    for (let right = rightCount - 1; right >= 0; right -= 1) {
      if (leftLines[left] === rightLines[right]) {
        matrix[base + right] = matrix[below + right + 1] + 1;
      } else {
        matrix[base + right] = Math.max(matrix[below + right], matrix[base + right + 1]);
      }
    }
    if (left % 32 === 0 && elapsed(startedAt) > WORK_BUDGET_MS) {
      return limited("time");
    }
  }

  const rows: DiffRow[] = [];
  let modifications = 0;
  let additions = 0;
  let deletions = 0;
  let leftIndex = 0;
  let rightIndex = 0;
  let pendingDeleted: DiffSourceLine[] = [];
  let pendingInserted: DiffSourceLine[] = [];

  const flushPending = () => {
    if (!pendingDeleted.length && !pendingInserted.length) return;
    const counts = appendRun(rows, pendingDeleted, pendingInserted);
    modifications += counts.modifications;
    additions += counts.additions;
    deletions += counts.deletions;
    pendingDeleted = [];
    pendingInserted = [];
  };

  while (leftIndex < leftCount || rightIndex < rightCount) {
    if (rows.length > MAX_RENDERED_ROWS) return limited("complexity");
    if ((leftIndex + rightIndex) % 128 === 0 && elapsed(startedAt) > WORK_BUDGET_MS) {
      return limited("time");
    }

    if (
      leftIndex < leftCount
      && rightIndex < rightCount
      && leftLines[leftIndex] === rightLines[rightIndex]
    ) {
      flushPending();
      rows.push({
        kind: "equal",
        left: { lineNumber: leftIndex + 1, text: leftLines[leftIndex] },
        right: { lineNumber: rightIndex + 1, text: rightLines[rightIndex] },
      });
      leftIndex += 1;
      rightIndex += 1;
      continue;
    }

    const removeScore = leftIndex < leftCount ? matrix[(leftIndex + 1) * width + rightIndex] : -1;
    const insertScore = rightIndex < rightCount ? matrix[leftIndex * width + rightIndex + 1] : -1;
    if (leftIndex < leftCount && (rightIndex >= rightCount || removeScore >= insertScore)) {
      pendingDeleted.push({ lineNumber: leftIndex + 1, text: leftLines[leftIndex] });
      leftIndex += 1;
    } else if (rightIndex < rightCount) {
      pendingInserted.push({ lineNumber: rightIndex + 1, text: rightLines[rightIndex] });
      rightIndex += 1;
    }
  }

  flushPending();
  return rows.length > MAX_RENDERED_ROWS
    ? limited("complexity")
    : { status: "ready", rows, modifications, additions, deletions };
}

export function sourceLines(content: string): DiffSourceLine[] {
  return splitLines(content).map((text, index) => ({ lineNumber: index + 1, text }));
}
