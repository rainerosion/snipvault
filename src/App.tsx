import { useState, useEffect, useLayoutEffect, useCallback, useMemo, useRef, useContext, Suspense } from "react";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { useSnippets } from "./hooks/useSnippets";
import {
  settingsToDraft,
  useSettings,
  type SyncCompletionEvent,
  type SyncSource,
} from "./hooks/useSettings";
import { Toolbar } from "./components/Toolbar";
import { Titlebar } from "./components/Titlebar";
import { Sidebar } from "./components/Sidebar";
import { CommandPalette, type CommandDefinition } from "./components/CommandPalette";
import { SnippetEditorLoadBoundary } from "./components/SnippetEditorLoadBoundary";
import { LazySnippetEditor } from "./components/LazySnippetEditor";
import { SettingsPanel, type SettingsPanelHandle } from "./components/Settings";
import { SyncNotificationCenter } from "./components/SyncNotificationCenter";
import { Dialog, DialogHandle } from "./components/Dialog";
import { Snippet, SnippetForm, SnippetSummary, type QuickCaptureCompletion, type SnippetSort } from "./types";
import { localizeCommandError, normalizeCommandError } from "./utils/commandErrors";
import { ThemeContext } from "./main";

const EMPTY_FORM: SnippetForm = {
  title: "",
  content: "",
  language: "plaintext",
  description: "",
  tags: [],
  is_favorite: false,
};

interface RevisionHistoryRestoreRequest {
  generation: number;
  snippet_id: string;
  target_revision_id: string;
}

function isFormDirty(current: SnippetForm, original: SnippetForm): boolean {
  return (
    current.title !== original.title ||
    current.content !== original.content ||
    current.language !== original.language ||
    current.description !== original.description ||
    JSON.stringify(current.tags) !== JSON.stringify(original.tags) ||
    current.is_favorite !== original.is_favorite
  );
}

function snippetToForm(snippet: Snippet): SnippetForm {
  return {
    title: snippet.title,
    content: snippet.content,
    language: snippet.language,
    description: snippet.description,
    tags: snippet.tags,
    is_favorite: snippet.is_favorite,
  };
}

export default function App() {
  const { t } = useTranslation();
  const {
    snippets,
    total,
    hasMore,
    tags: allTagOptions,
    loading,
    loadingMore,
    error,
    loadMoreError,
    load,
    loadMore,
    get,
    restoreRevision,
    create,
    update,
    remove,
    toggleFavorite,
    recordUsage,
    setManyFavorite,
    removeMany,
    exportAllToFile,
    importAll,
  } = useSnippets();
  const {
    sync,
    syncing,
    syncStatus,
    setSyncStatus,
    settings,
    reload: reloadSettings,
    save: saveSettings,
    reloadHistory,
    reloadNotifications,
    syncNotifications,
  } = useSettings();

  const [selected, setSelected] = useState<Snippet | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [form, setForm] = useState<SnippetForm>(EMPTY_FORM);
  const [editorLoadAttempt, setEditorLoadAttempt] = useState(0);
  const [saving, setSaving] = useState(false);
  const [detailState, setDetailState] = useState<
    | { status: "idle" }
    | { status: "loading"; summary: SnippetSummary }
    | { status: "error"; summary: SnippetSummary; error: unknown }
  >({ status: "idle" });
  const { theme, accentPreset, setTheme } = useContext(ThemeContext);
  const [searchQuery, setSearchQuery] = useState("");
  const [langFilter, setLangFilter] = useState("");
  const [favFilter, setFavFilter] = useState<boolean | null>(null);
  const [sort, setSort] = useState<SnippetSort>("updated");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [notificationsOpen, setNotificationsOpen] = useState(false);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [focusSearchAfterPaletteClose, setFocusSearchAfterPaletteClose] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [selectedIds, setSelectedIds] = useState<Set<string>>(() => new Set());
  const [bulkBusy, setBulkBusy] = useState(false);
  const [quickCaptureStatus, setQuickCaptureStatus] = useState<"success" | "failed" | null>(null);
  const quickCaptureStatusTimerRef = useRef<number | undefined>(undefined);
  const [refreshStatus, setRefreshStatus] = useState<"applied" | "stale" | null>(null);
  const lineWrap = settings?.editor_line_wrap ?? true;
  const [textMenu, setTextMenu] = useState<{ visible: boolean; x: number; y: number; isEditorContext: boolean }>({ visible: false, x: 0, y: 0, isEditorContext: false });
  const textMenuRef = useRef<HTMLDivElement | null>(null);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const textMenuTargetRef = useRef<HTMLElement | null>(null);
  const textMenuRestoreFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!focusSearchAfterPaletteClose || commandPaletteOpen) return;

    setFocusSearchAfterPaletteClose(false);
    searchInputRef.current?.focus();
  }, [commandPaletteOpen, focusSearchAfterPaletteClose]);

  const originalFormRef = useRef<SnippetForm>(EMPTY_FORM);
  const formRef = useRef<SnippetForm>(EMPTY_FORM);
  const editorTargetRef = useRef<string | null>(null);
  const saveRequestRef = useRef(0);
  const dialogRef = useRef<DialogHandle>(null);
  const settingsPanelRef = useRef<SettingsPanelHandle>(null);
  const isDirty = isFormDirty(form, originalFormRef.current);

  const detailRequestRef = useRef(0);
  const processedQuickCaptureIdsRef = useRef<Set<string>>(new Set());
  const handleEditorLoadRetry = useCallback(() => {
    setEditorLoadAttempt((attempt) => attempt + 1);
  }, []);
  const activeListRequest = useCallback(() => ({
    query: searchQuery,
    language: langFilter || null,
    favorite: favFilter,
    exact_tag: null,
    sort,
  }), [favFilter, langFilter, searchQuery, sort]);

  const reconcileAuthoritative = useCallback(async () => {
    const currentTarget = editorTargetRef.current;
    if (!currentTarget || currentTarget === "new") return "none" as const;
    if (isFormDirty(formRef.current, originalFormRef.current)) {
      setRefreshStatus("stale");
      return "preserve-dirty" as const;
    }

    const requestId = ++detailRequestRef.current;
    try {
      const latest = await get(currentTarget);
      if (requestId !== detailRequestRef.current || editorTargetRef.current !== currentTarget) {
        return "stale" as const;
      }
      if (isFormDirty(formRef.current, originalFormRef.current)) {
        setRefreshStatus("stale");
        return "preserve-dirty" as const;
      }
      const latestForm = snippetToForm(latest);
      setSelected(latest);
      setForm(latestForm);
      formRef.current = latestForm;
      originalFormRef.current = latestForm;
      setRefreshStatus("applied");
      return "refresh" as const;
    } catch (cause) {
      const normalized = normalizeCommandError(cause);
      if (
        requestId === detailRequestRef.current &&
        editorTargetRef.current === currentTarget &&
        normalized.code === "not_found"
      ) {
        setSelected(null);
        setIsNew(false);
        setForm(EMPTY_FORM);
        formRef.current = EMPTY_FORM;
        editorTargetRef.current = null;
        originalFormRef.current = EMPTY_FORM;
        setRefreshStatus("applied");
        return "clear" as const;
      }
      throw cause;
    }
  }, [get]);

  const reloadSnippets = useCallback(async () => {
    const authoritative = await load(activeListRequest());
    if (authoritative) await reconcileAuthoritative();
    return authoritative;
  }, [activeListRequest, load, reconcileAuthoritative]);

  const refreshAfterSync = useCallback(async () => {
    const [authoritative] = await Promise.all([
      reloadSnippets(),
      reloadSettings(),
      reloadHistory().catch(() => []),
      reloadNotifications().catch(() => []),
    ]);
    return authoritative;
  }, [reloadHistory, reloadNotifications, reloadSettings, reloadSnippets]);

  const reconcileSyncCompletion = useCallback(
    async (
      completion: SyncCompletionEvent,
      options: { showDialog: boolean },
    ): Promise<SyncCompletionEvent> => {
      setSyncStatus(completion);

      if (completion.status === "result" && completion.result?.success) {
        try {
          await refreshAfterSync();
        } catch (cause) {
          const reloadFailure: SyncCompletionEvent = {
            source: completion.source,
            status: "error",
            error: {
              code: "unknown",
              message: "The latest local state could not be reloaded.",
              retryable: true,
            },
          };
          setSyncStatus(reloadFailure);
          if (options.showDialog) {
            await dialogRef.current?.alert(
              t("errors.reloadAfterSyncFailed", {
                error: localizeCommandError(cause, t),
              }),
            );
          }
          return reloadFailure;
        }
      }

      if (completion.status !== "result" || !completion.result?.success) {
        void reloadNotifications().catch(() => {});
      }

      if (options.showDialog) {
        const text = completion.result?.message
          || (completion.error ? localizeCommandError(completion.error, t) : t("errors.syncFailedShort"));
        await dialogRef.current?.alert(text);
      }

      return completion;
    },
    [refreshAfterSync, reloadNotifications, setSyncStatus, t],
  );

  const runManualSync = useCallback(
    async (source: Extract<SyncSource, "toolbar" | "settings">): Promise<SyncCompletionEvent> => {
      try {
        const result = await sync(source);
        const completion: SyncCompletionEvent = {
          source,
          status: "result",
          result,
        };
        return await reconcileSyncCompletion(completion, {
          showDialog: source === "toolbar",
        });
      } catch (cause) {
        const normalized = normalizeCommandError(cause);
        const completion: SyncCompletionEvent = {
          source,
          status: normalized.code === "sync_busy" ? "busy" : "error",
          error: normalized,
        };
        return await reconcileSyncCompletion(completion, {
          showDialog: source === "toolbar",
        });
      }
    },
    [reconcileSyncCompletion, sync],
  );

  const handleSync = useCallback(async () => {
    let effectiveSettings = settings;

    if (!effectiveSettings) {
      try {
        effectiveSettings = await reloadSettings();
      } catch (cause) {
        await dialogRef.current?.alert(
          `${t("errors.settingsLoadFailed")} ${localizeCommandError(cause, t)}`,
        );
        return;
      }
    }

    if (!effectiveSettings.webdav_url.trim()) {
      await dialogRef.current?.alert(t("errors.noWebdav"));
      return;
    }

    const confirmed = await dialogRef.current?.confirm(t("settings.syncConfirm"));
    if (confirmed !== true) return;
    await runManualSync("toolbar");
  }, [reloadSettings, runManualSync, settings, t]);

  useEffect(() => {
    formRef.current = form;
  }, [form]);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    clearTimeout(searchTimer.current);
    setSelectedIds(new Set());
    searchTimer.current = setTimeout(() => {
      void reloadSnippets().catch(() => {});
    }, 150);
    return () => clearTimeout(searchTimer.current);
  }, [reloadSnippets]);

  const showQuickCaptureStatus = useCallback((status: "success" | "failed") => {
    clearTimeout(quickCaptureStatusTimerRef.current);
    setQuickCaptureStatus(status);
    quickCaptureStatusTimerRef.current = window.setTimeout(() => {
      setQuickCaptureStatus(null);
    }, 5000);
  }, []);

  useEffect(() => () => clearTimeout(quickCaptureStatusTimerRef.current), []);

  const handleQuickCaptureCompletion = useCallback(async (completion: QuickCaptureCompletion) => {
    if (completion.snippet_id) {
      if (processedQuickCaptureIdsRef.current.has(completion.snippet_id)) return;
      processedQuickCaptureIdsRef.current.add(completion.snippet_id);
    }

    if (!completion.success) {
      showQuickCaptureStatus("failed");
      return;
    }

    showQuickCaptureStatus("success");
    try {
      await reloadSnippets();
    } catch (cause) {
      await dialogRef.current?.alert(
        t("errors.reloadAfterMutationFailed", {
          error: localizeCommandError(cause, t),
        }),
      );
    }
  }, [reloadSnippets, showQuickCaptureStatus, t]);

  useEffect(() => {
    let disposed = false;
    const unlisteners: Array<() => void> = [];

    void (async () => {
      const tauriWindow = getCurrentWindow();
      const registered = await Promise.all([
        tauriWindow.listen<SyncCompletionEvent>("sync-complete", async (event) => {
          const source = event.payload.source;
          await reconcileSyncCompletion(event.payload, {
            showDialog: source === "tray",
          });
        }),
        tauriWindow.listen<QuickCaptureCompletion>("quick-capture-complete", (event) => {
          void invoke("take_quick_capture_completion").catch(() => {});
          void handleQuickCaptureCompletion(event.payload);
        }),
        tauriWindow.listen("open-settings", () => setSettingsOpen(true)),
        tauriWindow.listen("autostart-toggled", () => {
          void reloadSettings().catch(() => {});
        }),
      ]);

      if (disposed) {
        registered.forEach((unlisten) => unlisten());
        return;
      }

      unlisteners.push(...registered);
      const pending = await invoke<QuickCaptureCompletion | null>("take_quick_capture_completion");
      if (!disposed && pending) {
        await handleQuickCaptureCompletion(pending);
      }
    })().catch(() => {});

    return () => {
      disposed = true;
      unlisteners.forEach((unlisten) => unlisten());
    };
  }, [handleQuickCaptureCompletion, reconcileSyncCompletion, reloadSettings]);

  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      const target = e.composedPath().find((node) => {
        if (!(node instanceof HTMLElement)) return false;
        if (node instanceof HTMLTextAreaElement) return true;
        if (node instanceof HTMLInputElement) {
          return ["text", "search", "email", "url", "tel", "password", "number"].includes(
            node.type,
          );
        }
        if (node.isContentEditable) return true;
        return node.classList.contains("cm-editor") || !!node.closest(".cm-editor");
      }) as HTMLElement | undefined;

      if (!target) {
        textMenuTargetRef.current = null;
        setTextMenu((prev) => ({ ...prev, visible: false, isEditorContext: false }));
        return;
      }

      e.preventDefault();

      const textTarget: HTMLElement = target.classList.contains("cm-editor")
        ? target
        : target.closest<HTMLElement>(".cm-editor") ?? target;

      textMenuTargetRef.current = textTarget;
      textMenuRestoreFocusRef.current = textTarget;

      const isEditorContext = textTarget.classList.contains("cm-editor") || !!textTarget.closest(".cm-editor");
      const menuW = 132;
      const menuH = isEditorContext ? 184 : 148;
      const x = Math.max(8, Math.min(e.clientX, window.innerWidth - menuW - 8));
      const y = Math.max(8, Math.min(e.clientY, window.innerHeight - menuH - 8));

      setTextMenu({ visible: true, x, y, isEditorContext });
    };

    const hideMenu = (restoreFocus = false) => {
      const restoreTarget = restoreFocus ? textMenuRestoreFocusRef.current : null;
      setTextMenu((prev) => (prev.visible ? { ...prev, visible: false } : prev));
      if (
        restoreTarget?.isConnected &&
        !restoreTarget.inert &&
        restoreTarget.getAttribute("aria-hidden") !== "true"
      ) {
        restoreTarget.focus();
      }
    };

    const onPointerDown = (e: PointerEvent) => {
      const menu = textMenuRef.current;
      if (menu && e.target instanceof Node && menu.contains(e.target)) return;
      hideMenu();
    };

    const onKeyDown = (e: KeyboardEvent) => {
      const menu = textMenuRef.current;
      if (!menu) return;
      const items = Array.from(
        menu.querySelectorAll<HTMLButtonElement>("[role='menuitem']:not([disabled])"),
      );
      if (items.length === 0) return;
      const currentIndex = items.indexOf(document.activeElement as HTMLButtonElement);
      let nextIndex: number | null = null;

      if (e.key === "Escape") {
        e.preventDefault();
        hideMenu(true);
        return;
      }
      if (e.key === "Home") nextIndex = 0;
      else if (e.key === "End") nextIndex = items.length - 1;
      else if (e.key === "ArrowDown") nextIndex = (currentIndex + 1) % items.length;
      else if (e.key === "ArrowUp") nextIndex = (currentIndex - 1 + items.length) % items.length;

      if (nextIndex !== null) {
        e.preventDefault();
        items[nextIndex].focus();
      }
    };

    const onViewportChange = () => hideMenu();

    window.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", onViewportChange);
    window.addEventListener("scroll", onViewportChange, true);

    return () => {
      window.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", onViewportChange);
      window.removeEventListener("scroll", onViewportChange, true);
    };
  }, []);

  useLayoutEffect(() => {
    if (!textMenu.visible) return;
    textMenuRef.current
      ?.querySelector<HTMLButtonElement>("[role='menuitem']")
      ?.focus();
  }, [textMenu.visible]);

  const resetToEmpty = useCallback(() => {
    ++detailRequestRef.current;
    setSelected(null);
    setIsNew(false);
    setDetailState({ status: "idle" });
    setForm(EMPTY_FORM);
    formRef.current = EMPTY_FORM;
    editorTargetRef.current = null;
    originalFormRef.current = EMPTY_FORM;
    setRefreshStatus(null);
  }, []);

  const startNewSnippetDraft = useCallback(() => {
    ++detailRequestRef.current;
    setSelected(null);
    setIsNew(true);
    setDetailState({ status: "idle" });
    setForm(EMPTY_FORM);
    formRef.current = EMPTY_FORM;
    editorTargetRef.current = "new";
    originalFormRef.current = EMPTY_FORM;
    setRefreshStatus(null);
  }, []);

  const loadSnippet = useCallback((s: Snippet) => {
    const loaded = snippetToForm(s);
    setSelected(s);
    setIsNew(false);
    setDetailState({ status: "idle" });
    setForm(loaded);
    formRef.current = loaded;
    editorTargetRef.current = s.id;
    originalFormRef.current = loaded;
    setRefreshStatus(null);
  }, []);

  const handleSave = useCallback(async (): Promise<boolean> => {
    if (saving) return false;
    if (!form.title.trim()) {
      await dialogRef.current?.alert(t("snippet.titleRequired"));
      return false;
    }

    const submittedForm = form;
    const submittedTarget = isNew ? "new" : selected?.id ?? null;
    const requestId = ++saveRequestRef.current;
    formRef.current = submittedForm;
    editorTargetRef.current = submittedTarget;
    setSaving(true);
    try {
      let savedSnippet: Snippet;
      if (isNew) {
        savedSnippet = await create(submittedForm);
      } else if (selected) {
        savedSnippet = await update(selected.id, selected.revision_id, submittedForm);
      } else {
        return false;
      }

      // Do not overwrite a newer edit or navigation that happened while IPC was
      // pending. The database save succeeded, but the current UI now owns newer
      // state and must retain its dirty snapshot.
      if (
        requestId !== saveRequestRef.current
        || editorTargetRef.current !== submittedTarget
        || isFormDirty(formRef.current, submittedForm)
      ) {
        try {
          await reloadSnippets();
        } catch (reloadError) {
          await dialogRef.current?.alert(
            t("errors.reloadAfterMutationFailed", {
              error: localizeCommandError(reloadError, t),
            })
          );
        }
        return true;
      }

      const savedForm = snippetToForm(savedSnippet);
      setSelected(savedSnippet);
      setIsNew(false);
      setForm(savedForm);
      formRef.current = savedForm;
      editorTargetRef.current = savedSnippet.id;
      originalFormRef.current = savedForm;
      try {
        await reloadSnippets();
      } catch (reloadError) {
        await dialogRef.current?.alert(
          t("errors.reloadAfterMutationFailed", {
            error: localizeCommandError(reloadError, t),
          })
        );
      }
      return true;
    } catch (cause) {
      console.error(cause);
      const normalized = normalizeCommandError(cause);
      if (
        normalized.code === "stale_revision"
        && requestId === saveRequestRef.current
        && editorTargetRef.current === submittedTarget
        && selected
      ) {
        try {
          const latest = await get(selected.id);
          if (
            requestId === saveRequestRef.current
            && editorTargetRef.current === submittedTarget
          ) {
            setSelected(latest);
            setRefreshStatus("stale");
          }
        } catch {
          setRefreshStatus("stale");
        }
      }
      await dialogRef.current?.alert(
        t("errors.saveFailed", { error: localizeCommandError(cause, t) })
      );
      return false;
    } finally {
      if (requestId === saveRequestRef.current) {
        setSaving(false);
      }
    }
  }, [saving, isNew, form, selected, create, update, get, reloadSnippets, t]);

  const completeHistoryRestore = useCallback(async (
    generation: number,
    status: "succeeded" | "cancelled" | "failed",
  ) => {
    await invoke("complete_revision_history_restore", { generation, status });
  }, []);

  const handleHistoryRestoreRequest = useCallback(async (request: RevisionHistoryRestoreRequest) => {
    const targetIsSelected = selected?.id === request.snippet_id && !isNew;
    if (targetIsSelected && isDirty) {
      const action = await dialogRef.current?.ask(t("dialog.unsavedChanges"));
      if (action === "save") {
        const saved = await handleSave();
        if (!saved) {
          await completeHistoryRestore(request.generation, "cancelled");
          return;
        }
      } else if (action !== "discard") {
        await completeHistoryRestore(request.generation, "cancelled");
        return;
      }
    }

    const confirmed = await dialogRef.current?.confirm(t("snippet.restoreRevisionConfirm"));
    if (confirmed !== true) {
      await completeHistoryRestore(request.generation, "cancelled");
      return;
    }

    try {
      const current = await get(request.snippet_id);
      const restored = await restoreRevision(
        current.id,
        request.target_revision_id,
        current.revision_id,
      );
      if (editorTargetRef.current === restored.id) {
        loadSnippet(restored);
      }
      try {
        await reloadSnippets();
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.reloadAfterMutationFailed", {
            error: localizeCommandError(cause, t),
          }),
        );
      }
      await completeHistoryRestore(request.generation, "succeeded");
    } catch (cause) {
      const normalized = normalizeCommandError(cause);
      if (normalized.code === "stale_revision" && editorTargetRef.current === request.snippet_id) {
        try {
          const latest = await get(request.snippet_id);
          setSelected(latest);
          setRefreshStatus("stale");
        } catch {
          setRefreshStatus("stale");
        }
      }
      await completeHistoryRestore(request.generation, "failed").catch(() => {});
      await dialogRef.current?.alert(localizeCommandError(cause, t));
    }
  }, [completeHistoryRestore, get, handleSave, isDirty, isNew, loadSnippet, reloadSnippets, restoreRevision, selected, t]);

  const openRevisionHistory = useCallback(async () => {
    if (!selected || isNew) return;
    try {
      await invoke("open_revision_history", { id: selected.id });
    } catch (cause) {
      await dialogRef.current?.alert(localizeCommandError(cause, t));
    }
  }, [isNew, selected, t]);

  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;

    const processRequest = async (request: RevisionHistoryRestoreRequest) => {
      if (disposed) return;
      try {
        const mainWindow = getCurrentWindow();
        await mainWindow.show();
        await mainWindow.unminimize().catch(() => {});
        await mainWindow.setFocus().catch(() => {});
      } catch {
        // The confirmation flow remains safe if native focus restoration is unavailable.
      }
      await handleHistoryRestoreRequest(request);
    };

    const consumePendingRequest = async () => {
      const pending = await invoke<RevisionHistoryRestoreRequest | null>(
        "take_revision_history_restore_request",
      );
      if (pending) await processRequest(pending);
    };

    void getCurrentWindow()
      .listen("revision-history-restore-request", () => void consumePendingRequest())
      .then(async (registered) => {
        unlisten = registered;
        await consumePendingRequest();
      })
      .catch(() => {});

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [handleHistoryRestoreRequest]);


  useEffect(() => {
    void reloadNotifications().catch(() => {});
  }, [reloadNotifications]);

  const handleSnapshotRestore = useCallback(async (snapshotId: string): Promise<boolean> => {
    if (isDirty) {
      const action = await dialogRef.current?.ask(t("dialog.unsavedChanges"));
      if (action === "save") {
        const saved = await handleSave();
        if (!saved) return false;
      } else if (action !== "discard") {
        return false;
      }
    }

    const confirmed = await dialogRef.current?.confirm(t("snapshots.restoreConfirm"));
    if (confirmed !== true) return false;

    try {
      await invoke("restore_local_snapshot", { snapshotId });
    } catch (cause) {
      await dialogRef.current?.alert(localizeCommandError(cause, t));
      return false;
    }

    resetToEmpty();
    try {
      await Promise.all([
        reloadSnippets(),
        reloadSettings(),
        reloadHistory().catch(() => []),
        reloadNotifications().catch(() => []),
      ]);
    } catch (cause) {
      await dialogRef.current?.alert(
        t("errors.reloadAfterRestoreFailed", {
          error: localizeCommandError(cause, t),
        }),
      );
      return true;
    }

    await dialogRef.current?.alert(t("snapshots.restoreComplete"));
    return true;
  }, [handleSave, isDirty, reloadHistory, reloadNotifications, reloadSettings, reloadSnippets, resetToEmpty, t]);

  const fetchSnippetDetail = useCallback(async (snippet: SnippetSummary) => {
    const requestId = ++detailRequestRef.current;
    editorTargetRef.current = snippet.id;
    setSelected(null);
    setIsNew(false);
    setDetailState({ status: "loading", summary: snippet });
    setRefreshStatus(null);
    try {
      const detail = await get(snippet.id);
      if (requestId !== detailRequestRef.current || editorTargetRef.current !== snippet.id) return;
      loadSnippet(detail);
      void recordUsage(snippet.id)
        .then(() => (sort === "recent" ? reloadSnippets() : undefined))
        .catch(() => {});
    } catch (cause) {
      if (requestId !== detailRequestRef.current || editorTargetRef.current !== snippet.id) return;
      setDetailState({ status: "error", summary: snippet, error: cause });
    }
  }, [get, loadSnippet, recordUsage, reloadSnippets, sort]);

  const handleSelect = useCallback(async (snippet: SnippetSummary) => {
    if (selected?.id === snippet.id || (
      detailState.status !== "idle" && detailState.summary.id === snippet.id
    )) return;
    if (isDirty) {
      const action = await dialogRef.current?.ask(t("dialog.unsavedChanges"));
      if (action === "save") {
        const saved = await handleSave();
        if (!saved) return;
      } else if (action !== "discard") {
        return;
      }
    }

    await fetchSnippetDetail(snippet);
  }, [detailState, fetchSnippetDetail, handleSave, isDirty, selected, t]);

  const handleNew = useCallback(async () => {
    if (isDirty) {
      const action = await dialogRef.current?.ask(t("dialog.unsavedChanges"));
      if (action === "save") {
        const saved = await handleSave();
        if (!saved) return;
      } else if (action !== "discard") {
        return;
      }
    }
    startNewSnippetDraft();
  }, [isDirty, t, handleSave, startNewSnippetDraft]);

  const handleCancel = useCallback(async () => {
    if (isDirty) {
      const action = await dialogRef.current?.ask(t("dialog.unsavedChanges"));
      if (action === "save") {
        const saved = await handleSave();
        if (!saved) return;
      } else if (action !== "discard") {
        return;
      }
    }
    resetToEmpty();
  }, [isDirty, t, handleSave, resetToEmpty]);

  const handleDelete = useCallback(
    async (id: string) => {
      if (!(await dialogRef.current?.confirm(t("dialog.confirmDelete")))) return;
      try {
        await remove(id);
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.deleteFailed", { error: localizeCommandError(cause, t) })
        );
        return;
      }

      if (selected?.id === id) {
        resetToEmpty();
      }
      try {
        await reloadSnippets();
      } catch (reloadError) {
        await dialogRef.current?.alert(
          t("errors.reloadAfterMutationFailed", {
            error: localizeCommandError(reloadError, t),
          })
        );
      }
    },
    [remove, selected, reloadSnippets, resetToEmpty, t]
  );

  const handleToggleFav = useCallback(
    async (id: string) => {
      try {
        await toggleFavorite(id);
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.favoriteFailed", { error: localizeCommandError(cause, t) })
        );
        return;
      }

      try {
        await reloadSnippets();
      } catch (reloadError) {
        await dialogRef.current?.alert(
          t("errors.reloadAfterMutationFailed", {
            error: localizeCommandError(reloadError, t),
          })
        );
      }
    },
    [toggleFavorite, reloadSnippets, t]
  );

  const ensureBulkDraftSafety = useCallback(async (ids: string[]) => {
    const currentId = selected?.id ?? (isNew ? "new" : null);
    if (!isDirty || !currentId || currentId === "new" || !ids.includes(currentId)) return true;
    const action = await dialogRef.current?.ask(t("dialog.unsavedChanges"));
    if (action === "save") return handleSave();
    return action === "discard";
  }, [handleSave, isDirty, isNew, selected?.id, t]);

  const handleSetManyFavorite = useCallback(async (isFavorite: boolean) => {
    const ids = [...selectedIds];
    if (ids.length === 0 || bulkBusy) return;
    if (!(await ensureBulkDraftSafety(ids))) return;
    setBulkBusy(true);
    try {
      try {
        await setManyFavorite(ids, isFavorite);
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.bulkMutationFailed", { error: localizeCommandError(cause, t) }),
        );
        return;
      }

      setSelectedIds(new Set());
      try {
        await reloadSnippets();
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.reloadAfterMutationFailed", { error: localizeCommandError(cause, t) }),
        );
      }
    } finally {
      setBulkBusy(false);
    }
  }, [bulkBusy, ensureBulkDraftSafety, reloadSnippets, selectedIds, setManyFavorite, t]);

  const handleBulkDelete = useCallback(async () => {
    const ids = [...selectedIds];
    if (ids.length === 0 || bulkBusy) return;
    if (!(await ensureBulkDraftSafety(ids))) return;
    const confirmed = await dialogRef.current?.confirm(
      t("dialog.confirmBulkDelete", { count: ids.length }),
    );
    if (!confirmed) return;
    setBulkBusy(true);
    try {
      try {
        await removeMany(ids);
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.bulkMutationFailed", { error: localizeCommandError(cause, t) }),
        );
        return;
      }

      if (selected?.id && ids.includes(selected.id)) resetToEmpty();
      setSelectedIds(new Set());
      try {
        await reloadSnippets();
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.reloadAfterMutationFailed", { error: localizeCommandError(cause, t) }),
        );
      }
    } finally {
      setBulkBusy(false);
    }
  }, [bulkBusy, ensureBulkDraftSafety, reloadSnippets, removeMany, resetToEmpty, selected?.id, selectedIds, t]);

  const toggleBulkSelection = useCallback((id: string) => {
    setSelectedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else if (next.size < 200) next.add(id);
      return next;
    });
  }, []);

  const selectLoadedForBulk = useCallback(() => {
    setSelectedIds(new Set(snippets.slice(0, 200).map((snippet) => snippet.id)));
  }, [snippets]);

  const clearBulkSelection = useCallback(() => setSelectedIds(new Set()), []);

  const handleExport = useCallback(async () => {
    try {
      const result = await exportAllToFile();

      const successKey = result.saved_in_downloads
        ? "errors.exportSuccessDownloads"
        : "errors.exportSuccessFallback";

      const shouldOpen = await dialogRef.current?.confirm(
        t(successKey),
        "dialog.title",
        {
          cancelLabel: "errors.exportActionOk",
          confirmLabel: "errors.exportActionOpenFolder",
        }
      );
      if (shouldOpen) {
        await invoke("open_trusted_directory", { directory: "export" });
      }
    } catch (cause) {
      await dialogRef.current?.alert(
        t("errors.exportFailed", { error: localizeCommandError(cause, t) })
      );
    }
  }, [exportAllToFile, t]);

  const handleImportData = useCallback(
    async (jsonData: string) => {
      try {
        const result = await importAll(jsonData);
        const changed = result.inserted + result.updated;
        try {
          await reloadSnippets();
        } catch (reloadError) {
          await dialogRef.current?.alert(
            t("errors.reloadAfterMutationFailed", {
              error: localizeCommandError(reloadError, t),
            })
          );
          return;
        }
        await dialogRef.current?.alert(t("errors.importSuccess", { count: changed }));
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.importFailed") + ": " + localizeCommandError(cause, t)
        );
      }
    },
    [importAll, reloadSnippets, t]
  );

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.isComposing || event.repeat) return;
      const ctrl = event.ctrlKey || event.metaKey;
      if (!ctrl) return;

      const key = event.key.toLowerCase();
      if (key === "k") {
        event.preventDefault();
        event.stopPropagation();
        if (!settingsOpen && !dialogOpen) {
          setCommandPaletteOpen((open) => !open);
        }
        return;
      }
      if (settingsOpen || commandPaletteOpen || dialogOpen) {
        if (key === "n" || key === "s" || key === "e") event.preventDefault();
        return;
      }

      if (key === "n") {
        event.preventDefault();
        void handleNew();
      } else if (key === "s") {
        event.preventDefault();
        void handleSave();
      } else if (key === "e") {
        event.preventDefault();
        void handleExport();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [commandPaletteOpen, dialogOpen, handleNew, handleSave, handleExport, settingsOpen]);

  const handleThemeToggle = useCallback(() => {
    setTheme(theme === "dark" ? "light" : "dark");
  }, [theme, setTheme]);

  const handleToggleLineWrap = useCallback(async () => {
    let current = settings;
    if (!current) {
      try {
        current = await reloadSettings();
      } catch (cause) {
        await dialogRef.current?.alert(
          `${t("errors.settingsLoadFailed")} ${localizeCommandError(cause, t)}`,
        );
        return;
      }
    }

    try {
      await saveSettings({
        ...settingsToDraft(current),
        editor_line_wrap: !current.editor_line_wrap,
      });
    } catch (cause) {
      await dialogRef.current?.alert(
        t("errors.settingsFailed", { error: localizeCommandError(cause, t) }),
      );
    }
  }, [reloadSettings, saveSettings, settings, t]);

  const handleOpenSettings = useCallback(() => {
    setSettingsOpen(true);
  }, []);

  const commandDefinitions = useMemo<CommandDefinition[]>(() => [
    {
      id: "new",
      label: t("commandPalette.commands.new"),
      keywords: [t("snippet.new"), "new create"],
      shortcut: "Ctrl/Cmd+N",
      execute: handleNew,
    },
    {
      id: "save",
      label: t("commandPalette.commands.save"),
      keywords: [t("snippet.save"), "save"],
      shortcut: "Ctrl/Cmd+S",
      disabled: !isDirty || saving,
      execute: handleSave,
    },
    {
      id: "export",
      label: t("commandPalette.commands.export"),
      keywords: [t("toolbar.export"), "export backup"],
      shortcut: "Ctrl/Cmd+E",
      execute: handleExport,
    },
    {
      id: "sync",
      label: t("commandPalette.commands.sync"),
      keywords: [t("toolbar.sync"), "sync webdav"],
      disabled: syncing,
      execute: handleSync,
    },
    {
      id: "settings",
      label: t("commandPalette.commands.settings"),
      keywords: [t("toolbar.settings"), "settings preferences"],
      execute: handleOpenSettings,
    },
    {
      id: "focus-search",
      label: t("commandPalette.commands.focusSearch"),
      keywords: [t("search.placeholder"), "search find"],
      execute: () => {
        setFocusSearchAfterPaletteClose(true);
      },
    },
    {
      id: "theme",
      label: t("commandPalette.commands.toggleTheme"),
      keywords: [t("toolbar.toggleTheme"), "theme dark light"],
      execute: handleThemeToggle,
    },
    {
      id: "favorite",
      label: selected?.is_favorite
        ? t("commandPalette.commands.unfavorite")
        : t("commandPalette.commands.favorite"),
      keywords: [t("snippet.favorite"), t("snippet.unfavorite")],
      disabled: !selected || saving,
      execute: async () => {
        if (selected) await handleToggleFav(selected.id);
      },
    },
  ], [handleExport, handleNew, handleSave, handleSync, handleThemeToggle, handleToggleFav, handleOpenSettings, isDirty, saving, selected, syncing, t]);

  const focusTextTarget = useCallback(() => {
    const target = textMenuTargetRef.current;
    if (!target) return;
    target.focus();
  }, []);

  const resolveCodeMirrorView = useCallback(async (target: HTMLElement) => {
    if (!target.closest(".cm-editor")) return null;
    try {
      const { EditorView } = await import("@codemirror/view");
      return EditorView.findFromDOM(target);
    } catch {
      return null;
    }
  }, []);

  const runTextAction = useCallback(async (action: "cut" | "copy" | "paste" | "selectAll") => {
    const target = textMenuTargetRef.current;
    if (!target) return;

    focusTextTarget();

    const inputTarget = (target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement)
      ? target
      : null;
    const cm = inputTarget ? null : await resolveCodeMirrorView(target);
    if (!inputTarget && target.closest(".cm-editor") && !cm) {
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }
    if (cm) cm.focus();

    if (action === "selectAll") {
      if (inputTarget) {
        inputTarget.select();
      } else if (cm) {
        cm.dispatch({ selection: { anchor: 0, head: cm.state.doc.length } });
      } else {
        document.execCommand("selectAll");
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    if (action === "paste") {
      try {
        const txt = await readText();
        if (txt) {
          if (inputTarget) {
            const start = inputTarget.selectionStart ?? inputTarget.value.length;
            const end = inputTarget.selectionEnd ?? inputTarget.value.length;
            inputTarget.setRangeText(txt, start, end, "end");
            inputTarget.dispatchEvent(new Event("input", { bubbles: true }));
          } else if (cm) {
            cm.dispatch(cm.state.replaceSelection(txt));
          } else {
            document.execCommand("insertText", false, txt);
          }
        }
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.clipboardFailed", { error: localizeCommandError(cause, t) })
        );
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    if (inputTarget) {
      const start = inputTarget.selectionStart ?? 0;
      const end = inputTarget.selectionEnd ?? 0;
      const selectedText = inputTarget.value.slice(start, end);
      try {
        if (action === "copy") {
          if (selectedText) await writeText(selectedText);
        } else if (action === "cut" && selectedText) {
          await writeText(selectedText);
          inputTarget.setRangeText("", start, end, "start");
          inputTarget.dispatchEvent(new Event("input", { bubbles: true }));
        }
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.clipboardFailed", { error: localizeCommandError(cause, t) })
        );
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    if (cm) {
      const selectedText = cm.state.sliceDoc(cm.state.selection.main.from, cm.state.selection.main.to);
      try {
        if (action === "copy") {
          if (selectedText) await writeText(selectedText);
        } else if (action === "cut" && selectedText) {
          await writeText(selectedText);
          cm.dispatch(cm.state.replaceSelection(""));
        }
      } catch (cause) {
        await dialogRef.current?.alert(
          t("errors.clipboardFailed", { error: localizeCommandError(cause, t) })
        );
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    document.execCommand(action);
    setTextMenu((prev) => ({ ...prev, visible: false }));
  }, [focusTextTarget, resolveCodeMirrorView, t]);

  return (
    <>
      <Dialog ref={dialogRef} onOpenChange={setDialogOpen} />
      <CommandPalette
        open={commandPaletteOpen}
        commands={commandDefinitions}
        onClose={() => setCommandPaletteOpen(false)}
      />
      {notificationsOpen && (
        <SyncNotificationCenter
          onClose={() => setNotificationsOpen(false)}
          onSync={async () => {
            await handleSync();
          }}
        />
      )}
      {settingsOpen && (
        <div
          className="app-modal-layer"
          onClick={(event) => {
            if (event.target === event.currentTarget) {
              void settingsPanelRef.current?.requestClose();
            }
          }}
        >
          <SettingsPanel
            ref={settingsPanelRef}
            onClose={() => setSettingsOpen(false)}
            onSync={() => runManualSync("settings")}
            onRestoreSnapshot={handleSnapshotRestore}
          />
        </div>
      )}
      <div className={`app ${theme}`}>
      <div className="sync-live-region" role="status" aria-live="polite" aria-atomic="true">
        {syncStatus?.source === "background" && (
          syncStatus.status === "result" && syncStatus.result?.success
            ? t("settings.backgroundSyncSuccess")
            : syncStatus.status === "busy"
              ? t("commandErrors.sync_busy")
              : syncStatus.error
                ? localizeCommandError(syncStatus.error, t)
                : t("errors.syncFailedShort")
        )}
      </div>
      {quickCaptureStatus && (
        <div className={`toast ${quickCaptureStatus === "success" ? "success" : "error"}`} role="status">
          {t(quickCaptureStatus === "success" ? "quickCapture.success" : "quickCapture.failed")}
        </div>
      )}
      <Titlebar />
      <Toolbar
        searchQuery={searchQuery}
        onSearchChange={setSearchQuery}
        selectedLang={langFilter}
        onLangChange={setLangFilter}
        onNew={handleNew}
        onExport={handleExport}
        onImportData={handleImportData}
        onImportError={(msg) => dialogRef.current?.alert(msg)}
        theme={theme}
        onThemeToggle={handleThemeToggle}
        onFavoriteFilter={setFavFilter}
        sort={sort}
        onSortChange={setSort}
        onOpenCommandPalette={() => setCommandPaletteOpen(true)}
        searchInputRef={searchInputRef}
        onOpenSettings={handleOpenSettings}
        onOpenNotifications={() => setNotificationsOpen(true)}
        unreadNotifications={syncNotifications.filter((notification) => notification.read_at === null).length}
        onSync={handleSync}
        syncing={syncing}
        favoriteFilter={favFilter}
        totalCount={total}
      />

      <div className="app-main">
        <Sidebar
          snippets={snippets}
          selectedId={selected?.id ?? editorTargetRef.current}
          onSelect={handleSelect}
          onDelete={handleDelete}
          onToggleFavorite={handleToggleFav}
          selectedIds={selectedIds}
          onToggleSelection={toggleBulkSelection}
          onSelectLoaded={selectLoadedForBulk}
          onClearSelection={clearBulkSelection}
          onSetFavorite={handleSetManyFavorite}
          onBulkDelete={handleBulkDelete}
          bulkBusy={bulkBusy}
          loading={loading}
          loadingMore={loadingMore}
          hasMore={hasMore}
          error={error ? localizeCommandError(error, t) : null}
          loadMoreError={loadMoreError ? localizeCommandError(loadMoreError, t) : null}
          onRetry={() => { void reloadSnippets().catch(() => {}); }}
          onLoadMore={() => { void loadMore().catch(() => {}); }}
        />

        <div className="editor-pane">
          {refreshStatus && (
            <div
              className={`refresh-status ${refreshStatus === "stale" ? "warning" : ""}`}
              role="status"
            >
              {t(refreshStatus === "stale" ? "errors.remoteUpdatePending" : "errors.remoteUpdateApplied")}
            </div>
          )}
          {!selected && !isNew && detailState.status === "loading" ? (
            <div className="editor-empty" role="status">
              <div className="spinner" />
              <p>{t("snippet.loadingDetail")}</p>
            </div>
          ) : !selected && !isNew && detailState.status === "error" ? (
            <div className="editor-empty" role="alert">
              <p>{t("errors.loadDetailFailed", {
                error: localizeCommandError(detailState.error, t),
              })}</p>
              <button
                type="button"
                className="snippet-retry-btn"
                onClick={() => { void fetchSnippetDetail(detailState.summary); }}
              >
                {t("sidebar.retry")}
              </button>
            </div>
          ) : !selected && !isNew ? (
            <div className="editor-empty">
              <svg width="64" height="64" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1">
                <path d="M17 3a2.828 2.828 0 1 1 4 4L7.5 20.5 2 22l1.5-5.5L17 3z" />
              </svg>
              <p>{t("snippet.selectHint")}</p>
              <p className="hint">{t("snippet.shortcutHint")}</p>
            </div>
          ) : (
            <SnippetEditorLoadBoundary
              key={editorLoadAttempt}
              developmentHint={import.meta.env.DEV ? t("snippet.loadUnavailableDevHint") : undefined}
              onRetry={handleEditorLoadRetry}
              retryLabel={t("snippet.retryLoad")}
              title={t("snippet.loadUnavailable")}
            >
              <Suspense
                fallback={
                  <div className="editor-empty" role="status">
                    <div className="spinner" />
                    <p>{t("sidebar.loading")}</p>
                  </div>
                }
              >
                <LazySnippetEditor
                  attempt={editorLoadAttempt}
                  snippet={selected}
                  isNew={isNew}
                  form={form}
                  onChange={(f) => {
                    const next = { ...formRef.current, ...f };
                    formRef.current = next;
                    setForm(next);
                  }}
                  onSave={handleSave}
                  onCancel={handleCancel}
                  onOpenHistory={() => void openRevisionHistory()}
                  onClipboardError={(cause) => {
                    void dialogRef.current?.alert(
                      t("errors.clipboardFailed", { error: localizeCommandError(cause, t) })
                    );
                  }}
                  onCopied={() => {
                    if (!selected) return;
                    void recordUsage(selected.id)
                      .then(() => (sort === "recent" ? reloadSnippets() : undefined))
                      .catch(() => {});
                  }}
                  theme={theme}
                  accentPreset={accentPreset}
                  lineWrap={lineWrap}
                  saving={saving}
                  isDirty={isDirty}
                  tagOptions={allTagOptions}
                />
              </Suspense>
            </SnippetEditorLoadBoundary>
          )}
        </div>
      </div>
      </div>

      {textMenu.visible && (
        <div
          ref={textMenuRef}
          className="text-context-menu"
          style={{ left: textMenu.x, top: textMenu.y }}
          role="menu"
        >
          <button type="button" role="menuitem" className="text-context-item" onClick={() => runTextAction("cut")}>{t("contextMenu.cut")}</button>
          <button type="button" role="menuitem" className="text-context-item" onClick={() => runTextAction("copy")}>{t("contextMenu.copy")}</button>
          <button type="button" role="menuitem" className="text-context-item" onClick={() => runTextAction("paste")}>{t("contextMenu.paste")}</button>
          <div className="text-context-divider" role="separator" />
          <button type="button" role="menuitem" className="text-context-item" onClick={() => runTextAction("selectAll")}>{t("contextMenu.selectAll")}</button>
          {textMenu.isEditorContext && (
            <>
              <div className="text-context-divider" role="separator" />
              <button
                type="button"
                role="menuitem"
                className="text-context-item"
                aria-pressed={lineWrap}
                onClick={async () => {
                  await handleToggleLineWrap();
                  setTextMenu((prev) => ({ ...prev, visible: false }));
                }}
              >
                {lineWrap ? t("contextMenu.wrapOff") : t("contextMenu.wrapOn")}
              </button>
            </>
          )}
        </div>
      )}
    </>
  );
}
