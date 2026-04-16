import { useState, useEffect, useCallback, useRef, useContext } from "react";
import { invoke } from "@tauri-apps/api/core";
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager";
import { useTranslation } from "react-i18next";
import { useSnippets } from "./hooks/useSnippets";
import { useSettings, Settings as AppSettings } from "./hooks/useSettings";
import { Toolbar } from "./components/Toolbar";
import { Titlebar } from "./components/Titlebar";
import { Sidebar } from "./components/Sidebar";
import { SnippetEditor } from "./components/SnippetEditor";
import { SettingsPanel } from "./components/Settings";
import { Dialog, DialogHandle } from "./components/Dialog";
import { Snippet, SnippetForm } from "./types";
import { ThemeContext } from "./main";

const EMPTY_FORM: SnippetForm = {
  title: "",
  content: "",
  language: "javascript",
  description: "",
  tags: [],
  is_favorite: false,
};

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

export default function App() {
  const { t } = useTranslation();
  const {
    snippets,
    loading,
    load,
    create,
    update,
    remove,
    toggleFavorite,
    exportAll,
    importAll,
  } = useSnippets();
  const { syncUpload, settings, load: loadSettings } = useSettings();

  const [selected, setSelected] = useState<Snippet | null>(null);
  const [isNew, setIsNew] = useState(false);
  const [form, setForm] = useState<SnippetForm>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);
  const { theme, setTheme } = useContext(ThemeContext);
  const [searchQuery, setSearchQuery] = useState("");
  const [langFilter, setLangFilter] = useState("");
  const [favFilter, setFavFilter] = useState<boolean | null>(null);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [syncing, setSyncing] = useState(false);
  const [textMenu, setTextMenu] = useState<{ visible: boolean; x: number; y: number }>({ visible: false, x: 0, y: 0 });
  const textMenuRef = useRef<HTMLDivElement | null>(null);
  const textMenuTargetRef = useRef<HTMLElement | null>(null);

  const handleSync = useCallback(async () => {
    let effectiveSettings = settings;

    // Settings in this page may still be null/stale before loadSettings finishes.
    // Read once directly from backend as a fallback.
    if (!effectiveSettings?.webdav_url?.trim()) {
      try {
        effectiveSettings = await invoke<AppSettings>("get_settings");
      } catch {
        // keep fallback as null
      }
    }

    if (!effectiveSettings?.webdav_url?.trim()) {
      dialogRef.current?.alert(t("errors.noWebdav"));
      return;
    }

    const ok = await dialogRef.current?.confirm(t("settings.syncConfirm"));
    if (ok !== true) return;

    setSyncing(true);
    try {
      const result = await syncUpload();
      dialogRef.current?.alert(result.message);
      if (result.success) {
        await load();
        await loadSettings();
      }
    } catch (e) {
      dialogRef.current?.alert(t("errors.syncFailed", { error: e }));
    } finally {
      setSyncing(false);
    }
  }, [settings, syncUpload, load, loadSettings, t]);

  const originalFormRef = useRef<SnippetForm>(EMPTY_FORM);
  const dialogRef = useRef<DialogHandle>(null);
  const isDirty = isFormDirty(form, originalFormRef.current);
  const searchTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => { load(); }, [load]);

  useEffect(() => {
    (window as any).__openSettings = () => setSettingsOpen(true);
  }, []);

  useEffect(() => {
    import("@tauri-apps/api/window").then(({ getCurrentWindow }) => {
      const tauriWindow = getCurrentWindow() as any;
      const unlistenSync = tauriWindow.on("sync-complete", async (event: any) => {
        const result = event.payload as { success: boolean; message: string };
        setSyncing(false);
        dialogRef.current?.alert(result.message);
        if (result.success) {
          await load();
          await loadSettings();
        }
      });
      const unlistenSettings = tauriWindow.on("open-settings", () => { setSettingsOpen(true); });
      const unlistenAutoStart = tauriWindow.on("autostart-toggled", async () => {
        await loadSettings();
      });
      return () => {
        unlistenSync.then((f: () => void) => f());
        unlistenSettings.then((f: () => void) => f());
        unlistenAutoStart.then((f: () => void) => f());
      };
    }).catch(() => {});
  }, [setSettingsOpen]);

  useEffect(() => {
    document.getElementById("root")!.setAttribute("data-theme", theme);
  }, [theme]);

  useEffect(() => {
    clearTimeout(searchTimer.current);
    searchTimer.current = setTimeout(() => {
      if (langFilter || favFilter !== null || searchQuery) {
        const hasQuery = searchQuery.trim().length > 0;
        const hasFilter = langFilter || favFilter !== null;
        if (hasQuery || hasFilter) {
          const filtered = snippets.filter((s) => {
            const matchLang = !langFilter || s.language === langFilter;
            const matchFav = favFilter === null || s.is_favorite === favFilter;
            const matchSearch =
              !searchQuery ||
              s.title.toLowerCase().includes(searchQuery.toLowerCase()) ||
              s.content.toLowerCase().includes(searchQuery.toLowerCase()) ||
              s.description.toLowerCase().includes(searchQuery.toLowerCase()) ||
              s.tags.some((t) => t.toLowerCase().includes(searchQuery.toLowerCase()));
            return matchLang && matchFav && matchSearch;
          });
          if (JSON.stringify(filtered.map((s) => s.id)) !==
              JSON.stringify(snippets.map((s) => s.id))) {
            setFilteredSnippets(filtered);
          } else {
            setFilteredSnippets(null);
          }
        } else {
          setFilteredSnippets(null);
        }
      } else {
        setFilteredSnippets(null);
      }
    }, 150);
  }, [searchQuery, langFilter, favFilter, snippets]);

  const [filteredSnippets, setFilteredSnippets] = useState<Snippet[] | null>(null);
  const displaySnippets = filteredSnippets ?? snippets;
  const allTagOptions = Array.from(new Set(snippets.flatMap((s) => s.tags))).sort((a, b) => a.localeCompare(b));

  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const ctrl = e.ctrlKey || e.metaKey;
      if (ctrl && e.key === "n") {
        e.preventDefault();
        handleNew();
      }
      if (ctrl && e.key === "s") {
        e.preventDefault();
        handleSave();
      }
      if (ctrl && e.key === "e") {
        e.preventDefault();
        handleExport();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [form, selected, isNew]);

  useEffect(() => {
    const onContextMenu = (e: MouseEvent) => {
      const target = e.composedPath().find((node) => {
        if (!(node instanceof HTMLElement)) return false;
        if (node.matches("input, textarea, [contenteditable='true']")) return true;
        if (node.isContentEditable) return true;
        if (node.classList.contains("cm-editor")) return true;
        if (node.closest("input, textarea, [contenteditable='true'], .cm-editor")) return true;
        return false;
      }) as HTMLElement | undefined;

      e.preventDefault();

      if (!target) {
        textMenuTargetRef.current = null;
        setTextMenu((prev) => ({ ...prev, visible: false }));
        return;
      }

      const textTarget = target.matches("input, textarea, [contenteditable='true'], .cm-editor")
        ? target
        : target.closest("input, textarea, [contenteditable='true'], .cm-editor") as HTMLElement;

      textMenuTargetRef.current = textTarget;

      const menuW = 132;
      const menuH = 148;
      const x = Math.max(8, Math.min(e.clientX, window.innerWidth - menuW - 8));
      const y = Math.max(8, Math.min(e.clientY, window.innerHeight - menuH - 8));

      setTextMenu({ visible: true, x, y });
    };

    const hideMenu = () => {
      setTextMenu((prev) => (prev.visible ? { ...prev, visible: false } : prev));
    };

    const onPointerDown = (e: PointerEvent) => {
      const menu = textMenuRef.current;
      if (menu && e.target instanceof Node && menu.contains(e.target)) return;
      hideMenu();
    };

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") hideMenu();
    };

    window.addEventListener("contextmenu", onContextMenu);
    window.addEventListener("pointerdown", onPointerDown, true);
    window.addEventListener("keydown", onKeyDown);
    window.addEventListener("resize", hideMenu);
    window.addEventListener("scroll", hideMenu, true);

    return () => {
      window.removeEventListener("contextmenu", onContextMenu);
      window.removeEventListener("pointerdown", onPointerDown, true);
      window.removeEventListener("keydown", onKeyDown);
      window.removeEventListener("resize", hideMenu);
      window.removeEventListener("scroll", hideMenu, true);
    };
  }, []);

  const resetToEmpty = useCallback(() => {
    setSelected(null);
    setIsNew(false);
    setForm(EMPTY_FORM);
    originalFormRef.current = EMPTY_FORM;
  }, []);

  const startNewSnippetDraft = useCallback(() => {
    setSelected(null);
    setIsNew(true);
    setForm(EMPTY_FORM);
    originalFormRef.current = EMPTY_FORM;
  }, []);

  const loadSnippet = useCallback((s: Snippet) => {
    const loaded: SnippetForm = {
      title: s.title,
      content: s.content,
      language: s.language,
      description: s.description,
      tags: s.tags,
      is_favorite: s.is_favorite,
    };
    setSelected(s);
    setIsNew(false);
    setForm(loaded);
    originalFormRef.current = loaded;
  }, []);

  const handleSelect = useCallback((s: Snippet) => {
    if (selected?.id === s.id) return;
    if (isDirty) {
      dialogRef.current?.ask(t("dialog.unsavedChanges")).then((action) => {
        if (action === "save") {
          handleSave().then(() => loadSnippet(s));
        } else if (action === "discard") {
          loadSnippet(s);
        }
      });
      return;
    }
    loadSnippet(s);
  }, [isDirty, selected, loadSnippet, t]);

  const handleSave = useCallback(async () => {
    if (!form.title.trim()) {
      dialogRef.current?.alert(t("snippet.titleRequired"));
      return;
    }
    setSaving(true);
    try {
      if (isNew) {
        await create(form);
        setIsNew(false);
        await load();
      } else if (selected) {
        await update(selected.id, form, selected.updated_at);
        setSelected({ ...selected, ...form, updated_at: new Date().toISOString() });
        await load();
      }
      originalFormRef.current = { ...form };
    } catch (err) {
      console.error(err);
      dialogRef.current?.alert(t("errors.saveFailed", { error: err }));
    } finally {
      setSaving(false);
    }
  }, [isNew, form, selected, create, update, load, t]);

  const handleNew = useCallback(() => {
    if (isDirty) {
      dialogRef.current?.ask(t("dialog.unsavedChanges")).then((action) => {
        if (action === "save") {
          handleSave().then(() => startNewSnippetDraft());
        } else if (action === "discard") {
          startNewSnippetDraft();
        }
      });
      return;
    }
    startNewSnippetDraft();
  }, [isDirty, t, handleSave, startNewSnippetDraft]);

  const handleCancel = useCallback(() => {
    if (isDirty) {
      dialogRef.current?.ask(t("dialog.unsavedChanges")).then((action) => {
        if (action === "save") {
          handleSave().then(() => resetToEmpty());
        } else if (action === "discard") {
          resetToEmpty();
        }
      });
      return;
    }
    resetToEmpty();
  }, [isDirty, t]);

  const handleDelete = useCallback(
    async (id: string) => {
      if (!(await dialogRef.current?.confirm(t("dialog.confirmDelete")))) return;
      await remove(id);
      if (selected?.id === id) {
        resetToEmpty();
      }
      await load();
    },
    [remove, selected, load, resetToEmpty, t]
  );

  const handleToggleFav = useCallback(
    async (id: string) => { await toggleFavorite(id); },
    [toggleFavorite]
  );

  const handleExport = useCallback(async () => {
    const json = await exportAll();
    const blob = new Blob([json], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `gist-backup-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(url);
  }, [exportAll]);

  const handleImportData = useCallback(
    async (jsonData: string) => {
      try {
        const count = await importAll(jsonData);
        dialogRef.current?.alert(t("errors.importSuccess", { count }));
        await load();
      } catch (err) {
        dialogRef.current?.alert(t("errors.importFailed") + ": " + err);
      }
    },
    [importAll, load, t]
  );

  const handleThemeToggle = useCallback(() => {
    setTheme(theme === "dark" ? "light" : "dark");
  }, [theme, setTheme]);

  const handleOpenSettings = useCallback(() => {
    setSettingsOpen(true);
  }, []);

  const focusTextTarget = useCallback(() => {
    const target = textMenuTargetRef.current;
    if (!target) return;
    target.focus();
  }, []);

  const runTextAction = useCallback(async (action: "cut" | "copy" | "paste" | "selectAll") => {
    const target = textMenuTargetRef.current;
    if (!target) return;

    focusTextTarget();

    const isInput = target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement;
    const isCm = target.classList.contains("cm-editor") || !!target.closest(".cm-editor");

    if (action === "selectAll") {
      if (isInput) {
        target.select();
      } else if (isCm) {
        const el = target.classList.contains("cm-editor") ? target : target.closest(".cm-editor") as HTMLElement;
        const cm = (el as any).cmView?.view;
        if (cm) {
          cm.dispatch({ selection: { anchor: 0, head: cm.state.doc.length } });
        }
      } else {
        document.execCommand("selectAll");
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    if (action === "paste") {
      const txt = await readText().catch(() => "");
      if (txt) {
        if (isInput) {
          const start = target.selectionStart ?? target.value.length;
          const end = target.selectionEnd ?? target.value.length;
          target.setRangeText(txt, start, end, "end");
          target.dispatchEvent(new Event("input", { bubbles: true }));
        } else if (isCm) {
          const el = target.classList.contains("cm-editor") ? target : target.closest(".cm-editor") as HTMLElement;
          const cm = (el as any).cmView?.view;
          if (cm) cm.dispatch(cm.state.replaceSelection(txt));
        } else {
          document.execCommand("insertText", false, txt);
        }
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    if (isInput) {
      const start = target.selectionStart ?? 0;
      const end = target.selectionEnd ?? 0;
      const selected = target.value.slice(start, end);
      if (action === "copy") {
        if (selected) await writeText(selected).catch(() => {});
      } else if (action === "cut") {
        if (selected) {
          await writeText(selected).catch(() => {});
          target.setRangeText("", start, end, "start");
          target.dispatchEvent(new Event("input", { bubbles: true }));
        }
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    if (isCm) {
      const el = target.classList.contains("cm-editor") ? target : target.closest(".cm-editor") as HTMLElement;
      const cm = (el as any).cmView?.view;
      if (cm) {
        const selected = cm.state.sliceDoc(cm.state.selection.main.from, cm.state.selection.main.to);
        if (action === "copy") {
          if (selected) await writeText(selected).catch(() => {});
        } else if (action === "cut") {
          if (selected) {
            await writeText(selected).catch(() => {});
            cm.dispatch(cm.state.replaceSelection(""));
          }
        }
      }
      setTextMenu((prev) => ({ ...prev, visible: false }));
      return;
    }

    document.execCommand(action);
    setTextMenu((prev) => ({ ...prev, visible: false }));
  }, [focusTextTarget]);

  return (
    <>
      <Dialog ref={dialogRef} theme={theme} />
      {settingsOpen && (
        <div className="settings-overlay" onClick={(e) => { if (e.target === e.currentTarget) setSettingsOpen(false); }}>
          <SettingsPanel theme={theme} setTheme={setTheme} onClose={() => setSettingsOpen(false)} />
        </div>
      )}
      <div className={`app ${theme}`}>
      <Titlebar theme={theme} />
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
        onOpenSettings={handleOpenSettings}
        onSync={handleSync}
        syncing={syncing}
        favoriteFilter={favFilter}
        totalCount={displaySnippets.length}
      />

      <div className="app-main">
        <Sidebar
          snippets={displaySnippets}
          selectedId={selected?.id ?? null}
          onSelect={handleSelect}
          onDelete={handleDelete}
          onToggleFavorite={handleToggleFav}
          loading={loading}
        />

        <div className="editor-pane">
          <SnippetEditor
            snippet={selected}
            isNew={isNew}
            form={form}
            onChange={(f) => setForm((prev) => ({ ...prev, ...f }))}
            onSave={handleSave}
            onCancel={handleCancel}
            theme={theme}
            saving={saving}
            isDirty={isDirty}
            tagOptions={allTagOptions}
          />
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
          <button type="button" className="text-context-item" onClick={() => runTextAction("cut")}>剪切</button>
          <button type="button" className="text-context-item" onClick={() => runTextAction("copy")}>复制</button>
          <button type="button" className="text-context-item" onClick={() => runTextAction("paste")}>粘贴</button>
          <div className="text-context-divider" />
          <button type="button" className="text-context-item" onClick={() => runTextAction("selectAll")}>全选</button>
        </div>
      )}
    </>
  );
}
