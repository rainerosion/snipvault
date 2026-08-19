import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useCallback, useContext, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ThemeContext } from "../main";
import type {
  RevisionComparison,
  RevisionContent,
  RevisionPage,
} from "../types";
import {
  RevisionHistory,
  type RevisionHistoryRestoreOutcome,
  type RevisionHistoryTarget,
} from "./RevisionHistory";
import { Titlebar } from "./Titlebar";

export function RevisionHistoryWindow() {
  const { t } = useTranslation();
  const { theme } = useContext(ThemeContext);
  const [target, setTarget] = useState<RevisionHistoryTarget | null>(null);
  const [outcome, setOutcome] = useState<RevisionHistoryRestoreOutcome | null>(null);
  const [error, setError] = useState<unknown>(null);
  const latestTargetGenerationRef = useRef(0);

  const applyTarget = useCallback((next: RevisionHistoryTarget) => {
    if (next.generation < latestTargetGenerationRef.current) return;
    latestTargetGenerationRef.current = next.generation;
    setTarget((current) => current?.generation === next.generation ? current : next);
    setOutcome(null);
    setError(null);
  }, []);

  const refreshTarget = useCallback(async () => {
    try {
      const next = await invoke<RevisionHistoryTarget | null>("get_revision_history_target");
      if (next) applyTarget(next);
    } catch (cause) {
      setError(cause);
    }
  }, [applyTarget]);

  const refreshOutcome = useCallback(async () => {
    try {
      const next = await invoke<RevisionHistoryRestoreOutcome | null>("get_revision_history_restore_outcome");
      if (next) setOutcome(next);
    } catch {
      // The next target refresh or explicit event will safely resynchronize the child.
    }
  }, []);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void Promise.all([
      getCurrentWindow().listen<RevisionHistoryTarget>("revision-history-target-changed", (event) => {
        if (!disposed) applyTarget(event.payload);
      }),
      getCurrentWindow().listen<RevisionHistoryRestoreOutcome>("revision-history-restore-outcome", (event) => {
        if (!disposed) setOutcome(event.payload);
      }),
      getCurrentWindow().onFocusChanged((event) => {
        if (event.payload) {
          void refreshTarget();
          void refreshOutcome();
        }
      }),
    ]).then((registered) => {
      if (disposed) {
        registered.forEach((unlisten) => unlisten());
        return;
      }
      unlisteners.push(...registered);
      void refreshTarget();
      void refreshOutcome();
    }).catch((cause) => {
      if (!disposed) setError(cause);
    });

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [applyTarget, refreshOutcome, refreshTarget]);

  const requestRestore = useCallback(async (targetRevisionId: string) => {
    if (!target) return;
    await invoke("request_revision_history_restore", {
      generation: target.generation,
      targetRevisionId,
    });
  }, [target]);

  const loadPage = useCallback(
    (cursor: string | null) => {
      if (!target) return Promise.reject(new Error("No revision-history target is available."));
      return invoke<RevisionPage>("list_snippet_revisions", { id: target.snippet_id, cursor, limit: 30 });
    },
    [target],
  );
  const loadRevision = useCallback(
    (revisionId: string) => {
      if (!target) return Promise.reject(new Error("No revision-history target is available."));
      return invoke<RevisionContent>("get_snippet_revision", { id: target.snippet_id, revisionId });
    },
    [target],
  );
  const compare = useCallback(
    (leftRevisionId: string, rightRevisionId: string) => {
      if (!target) return Promise.reject(new Error("No revision-history target is available."));
      return invoke<RevisionComparison>("compare_snippet_revisions", {
        id: target.snippet_id,
        leftRevisionId,
        rightRevisionId,
      });
    },
    [target],
  );

  return (
    <div className="revision-history-window-root">
      <Titlebar title={t("snippet.history")} />
      {error ? (
        <main className="revision-history-window revision-history-window-empty">
          <p className="revision-history-error" role="alert">{t("snippet.historyWindowUnavailable")}</p>
        </main>
      ) : target ? (
        <RevisionHistory
          key={target.generation}
          target={target}
          theme={theme}
          loadPage={loadPage}
          loadRevision={loadRevision}
          compare={compare}
          onRestore={requestRestore}
          restoreOutcome={outcome}
        />
      ) : (
        <main className="revision-history-window revision-history-window-empty">
          <p className="revision-history-empty" role="status">{t("snippet.historyLoading")}</p>
        </main>
      )}
    </div>
  );
}
