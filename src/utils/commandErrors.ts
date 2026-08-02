import type { TFunction } from "i18next";

export const COMMAND_ERROR_CODES = [
  "validation",
  "not_found",
  "stale_revision",
  "outbox_full",
  "database",
  "settings",
  "network",
  "sync_busy",
  "sync_cas_conflict",
  "sync_legacy_changed",
  "import",
  "export",
  "autostart",
  "credential",
  "recovery",
  "open",
  "unknown",
] as const;

export type CommandErrorCode = (typeof COMMAND_ERROR_CODES)[number];

export interface CommandError {
  code: CommandErrorCode;
  message: string;
  retryable: boolean;
  details?: Record<string, string>;
}

const COMMAND_ERROR_CODE_SET = new Set<string>(COMMAND_ERROR_CODES);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseCommandError(value: unknown): CommandError | null {
  if (!isRecord(value)) return null;
  if (
    typeof value.code !== "string" ||
    !COMMAND_ERROR_CODE_SET.has(value.code) ||
    typeof value.message !== "string" ||
    typeof value.retryable !== "boolean"
  ) {
    return null;
  }

  const details = isRecord(value.details)
    ? Object.fromEntries(
        Object.entries(value.details).filter(
          (entry): entry is [string, string] => typeof entry[1] === "string"
        )
      )
    : undefined;

  return {
    code: value.code as CommandErrorCode,
    message: value.message,
    retryable: value.retryable,
    ...(details && Object.keys(details).length > 0 ? { details } : {}),
  };
}

export function normalizeCommandError(value: unknown): CommandError {
  const direct = parseCommandError(value);
  if (direct) return direct;

  if (typeof value === "string") {
    try {
      const parsed = parseCommandError(JSON.parse(value));
      if (parsed) return parsed;
    } catch {
      // Legacy and malformed strings must not be reflected back to the user.
    }
  }

  return {
    code: "unknown",
    message: "The operation could not be completed.",
    retryable: false,
  };
}

export function localizeCommandError(value: unknown, t: TFunction): string {
  const error = normalizeCommandError(value);
  return t(`commandErrors.${error.code}`, {
    defaultValue: t("commandErrors.unknown"),
  });
}
